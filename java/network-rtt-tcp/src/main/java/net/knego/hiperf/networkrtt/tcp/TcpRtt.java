package net.knego.hiperf.networkrtt.tcp;

import java.io.EOFException;
import java.io.IOException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.StandardSocketOptions;
import java.nio.ByteBuffer;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;

/**
 * TCP ping-pong RTT transport using NIO non-blocking {@link SocketChannel} with
 * busy-poll on the client and bounded-spin on the responder, to eliminate the
 * blocking-read park/wakeup that dominates loopback RTT.
 *
 * <p>CLIENT: hot-spins on the receive side (unbounded) so there is no thread
 * park between sending and receiving the echo. RESPONDER: bounded spin
 * ({@link #SPIN_BUDGET} iterations) then falls back to a single blocking read
 * to yield the CPU when idle — this prevents the +45× p99 regression seen with
 * an unbounded responder spin.
 *
 * <p>{@link #loopback} wires an in-process server on an ephemeral 127.0.0.1
 * port to a client.
 */
final class TcpRtt {

    private TcpRtt() {}

    /** Maximum non-blocking read attempts before the responder falls back to a blocking read. */
    private static final int SPIN_BUDGET = 1000;

    /**
     * Loopback mode: start an in-process echo server on an ephemeral 127.0.0.1
     * port and run the client against it, returning the measured samples.
     */
    static long[] loopback(Config cfg) throws Exception {
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (ServerSocketChannel serverChannel = ServerSocketChannel.open()) {
            serverChannel.bind(new InetSocketAddress(loopback, 0));
            int port = ((InetSocketAddress) serverChannel.getLocalAddress()).getPort();

            Thread serverThread = new Thread(() -> runEchoServer(serverChannel, cfg.payloadBytes()),
                    "tcp-echo-server");
            serverThread.setDaemon(true);
            serverThread.start();

            long[] samples = client(loopback, port, cfg);
            serverThread.join(2000);
            return samples;
        }
    }

    /**
     * Server/responder: bind {@code address:port} and echo fixed-size payloads
     * for every accepted connection until the process is killed. Each
     * connection is served on its own daemon thread. Logs to stderr.
     */
    static void serve(InetAddress address, int port, int payloadBytes) throws IOException {
        try (ServerSocketChannel serverChannel = ServerSocketChannel.open()) {
            serverChannel.bind(new InetSocketAddress(address, port));
            System.err.println("network-rtt: TCP responder listening on "
                    + address.getHostAddress() + ":" + port);
            while (true) {
                SocketChannel conn = serverChannel.accept();
                Thread t = new Thread(() -> serveConnection(conn, payloadBytes),
                        "tcp-echo-conn");
                t.setDaemon(true);
                t.start();
            }
        }
    }

    /**
     * Client measurement loop: connect to {@code host:port} (Nagle disabled),
     * configure non-blocking, run warmup, then time {@code RTT_ITERATIONS}
     * synchronous round trips with an echo-byte equality assertion.
     * Returns the measured samples.
     */
    static long[] client(InetAddress host, int port, Config cfg) throws Exception {
        SocketChannel ch = SocketChannel.open();
        ch.connect(new InetSocketAddress(host, port));
        ch.setOption(StandardSocketOptions.TCP_NODELAY, true);
        ch.configureBlocking(false);
        try {
            return runClient(ch, cfg);
        } finally {
            ch.close();
        }
    }

    /** Convenience overload resolving {@code cfg.host()} for client mode. */
    static long[] client(Config cfg) throws Exception {
        InetAddress host = InetAddress.getByName(cfg.host());
        return client(host, cfg.tcpPort(), cfg);
    }

    /**
     * Echoes a fixed-size payload back for every round trip until the client
     * closes. Uses a bounded spin on reads to yield the CPU when idle.
     */
    private static void serveConnection(SocketChannel conn, int payloadBytes) {
        try (SocketChannel ch = conn) {
            ch.setOption(StandardSocketOptions.TCP_NODELAY, true);
            ch.configureBlocking(false);
            ByteBuffer buf = ByteBuffer.allocateDirect(payloadBytes);
            while (true) {
                // Bounded-spin read
                buf.clear();
                int spins = 0;
                while (buf.hasRemaining()) {
                    int r = ch.read(buf);
                    if (r < 0) {
                        return; // clean disconnect
                    }
                    if (r == 0) {
                        if (++spins >= SPIN_BUDGET) {
                            ch.configureBlocking(true);
                            int b = ch.read(buf);
                            ch.configureBlocking(false);
                            if (b < 0) {
                                return;
                            }
                            spins = 0;
                        } else {
                            Thread.onSpinWait();
                        }
                    }
                }
                // Write echo
                buf.flip();
                while (buf.hasRemaining()) {
                    ch.write(buf);
                }
            }
        } catch (IOException e) {
            // Connection closed / reset on client shutdown is expected at end of run.
        }
    }

    /** Echoes a single accepted connection (used by the in-process loopback server). */
    private static void runEchoServer(ServerSocketChannel server, int payloadBytes) {
        try {
            serveConnection(server.accept(), payloadBytes);
        } catch (IOException e) {
            // Server socket closed at end of run is expected.
        }
    }

    private static long[] runClient(SocketChannel ch, Config cfg) throws Exception {
        int n = cfg.payloadBytes();

        // Build the send payload (same pattern as before: byte at index i = i & 0xFF)
        byte[] payload = new byte[n];
        for (int i = 0; i < n; i++) {
            payload[i] = (byte) (i & 0xFF);
        }

        // Allocate direct ByteBuffers once; reused across the entire measured loop (no per-trip alloc)
        ByteBuffer sendBuf = ByteBuffer.allocateDirect(n);
        sendBuf.put(payload);
        // sendBuf: position=n, limit=n — flip() on first iteration makes it ready to drain

        ByteBuffer recvBuf = ByteBuffer.allocateDirect(n);
        // recvBuf: position=0, limit=n — clear() on first iteration leaves it ready to fill

        // Build a read-only expected buffer once; never mutated, stays reusable across iterations
        ByteBuffer expectedBuf = ByteBuffer.allocateDirect(n);
        expectedBuf.put(payload);
        expectedBuf.flip();
        // expectedBuf: position=0, limit=n (read-only view for mismatch comparisons)

        return Measure.run(cfg, () -> roundTrip(ch, sendBuf, recvBuf, expectedBuf, n));
    }

    /**
     * One synchronous round trip over a non-blocking SocketChannel.
     *
     * <p>Buffer invariants at entry and exit so the buffers are reusable across iterations:
     * <ul>
     *   <li>{@code sendBuf}: position=n, limit=n — {@code flip()} makes it ready to write</li>
     *   <li>{@code recvBuf}: position=n, limit=n — {@code clear()} makes it ready to fill</li>
     * </ul>
     */
    private static void roundTrip(SocketChannel ch, ByteBuffer sendBuf, ByteBuffer recvBuf,
            ByteBuffer expectedBuf, int n) throws IOException {
        // flip(): position=0, limit=n — drain entire payload into the channel
        sendBuf.flip();
        while (sendBuf.hasRemaining()) {
            ch.write(sendBuf);
        }
        // sendBuf: position=n, limit=n (invariant restored)

        // Unbounded hot-spin read: fills recvBuf completely before returning
        recvBuf.clear();
        while (recvBuf.hasRemaining()) {
            int r = ch.read(recvBuf);
            if (r < 0) {
                throw new EOFException("TCP echo server closed mid-round-trip");
            }
            if (r == 0) {
                Thread.onSpinWait();
            }
        }
        // recvBuf: position=n, limit=n

        // Verify echo equality via intrinsified bulk compare; mismatch() does not advance either
        // buffer's position, so expectedBuf stays reusable across iterations.
        recvBuf.flip(); // position=0, limit=n — prepare for comparison
        if (recvBuf.mismatch(expectedBuf) >= 0) {
            throw new IOException("TCP echo mismatch: received bytes differ from sent");
        }
        recvBuf.position(n); // restore pos=n, limit=n invariant for next iteration's clear()
        // recvBuf: position=n, limit=n (invariant restored)
    }
}
