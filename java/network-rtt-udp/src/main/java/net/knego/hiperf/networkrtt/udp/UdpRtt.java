package net.knego.hiperf.networkrtt.udp;

import java.io.IOException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.ClosedChannelException;
import java.nio.channels.DatagramChannel;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;

/**
 * UDP ping-pong RTT transport using NIO DatagramChannel busy-poll.
 *
 * <p>The client hot-spins on read (non-blocking channel), bounded by a ~1s deadline that throws
 * a loss error. The responder bounded-spins on receive, then falls back to a blocking receive —
 * it never errors on idle. {@link #loopback} wires an in-process server on an ephemeral
 * 127.0.0.1 port to a client.
 */
final class UdpRtt {

    private static final int SO_TIMEOUT_MS = 1000;
    private static final int SPIN_BUDGET = 1000;

    private UdpRtt() {}

    /**
     * Loopback mode: start an in-process echo server on an ephemeral 127.0.0.1 port and run the
     * client against it, returning the measured samples.
     */
    static long[] loopback(Config cfg) throws Exception {
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (DatagramChannel serverCh = DatagramChannel.open()) {
            serverCh.bind(new InetSocketAddress(loopback, 0));
            serverCh.configureBlocking(false);
            int port = ((InetSocketAddress) serverCh.getLocalAddress()).getPort();

            Thread serverThread = new Thread(() -> runEchoServer(serverCh, cfg.payloadBytes()),
                    "udp-echo-server");
            serverThread.setDaemon(true);
            serverThread.start();

            try {
                return client(loopback, port, cfg);
            } finally {
                serverCh.close(); // unblock the server's blocking-fallback receive
                serverThread.join(2000);
            }
        }
    }

    /**
     * Server/responder: bind {@code address:port} and echo each received datagram back to its
     * sender until the process is killed. Logs to stderr.
     */
    static void serve(InetAddress address, int port, int payloadBytes) throws IOException {
        try (DatagramChannel serverCh = DatagramChannel.open()) {
            serverCh.bind(new InetSocketAddress(address, port));
            serverCh.configureBlocking(false);
            System.err.println("network-rtt: UDP responder listening on "
                    + address.getHostAddress() + ":" + port);
            runEchoServer(serverCh, payloadBytes);
        }
    }

    /**
     * Client measurement loop: connect a non-blocking datagram channel to {@code host:port},
     * busy-poll on receive (1s deadline = hard loss error), run warmup, then time
     * {@code RTT_ITERATIONS} synchronous round trips with an echo-byte equality assertion.
     * Returns the measured samples.
     */
    static long[] client(InetAddress host, int port, Config cfg) throws Exception {
        try (DatagramChannel ch = DatagramChannel.open()) {
            ch.connect(new InetSocketAddress(host, port));
            ch.configureBlocking(false);
            return runClient(ch, cfg);
        }
    }

    /** Convenience overload resolving {@code cfg.host()} for client mode. */
    static long[] client(Config cfg) throws Exception {
        InetAddress host = InetAddress.getByName(cfg.host());
        return client(host, cfg.udpPort(), cfg);
    }

    /**
     * Echoes each received datagram back to its sender. Bounded-spins on receive, then falls back
     * to a blocking receive so the thread parks on idle; never errors on idle. Exits cleanly when
     * the channel is closed.
     */
    private static void runEchoServer(DatagramChannel ch, int payloadBytes) {
        ByteBuffer buf = ByteBuffer.allocateDirect(payloadBytes);
        try {
            while (true) {
                buf.clear();
                int spins = 0;
                SocketAddress from = ch.receive(buf); // null when nothing pending (non-blocking)
                while (from == null) {
                    if (++spins >= SPIN_BUDGET) {
                        ch.configureBlocking(true);
                        buf.clear();
                        from = ch.receive(buf); // blocking: waits for next datagram (idle = wait, not error)
                        ch.configureBlocking(false);
                        break;
                    }
                    Thread.onSpinWait();
                    buf.clear();
                    from = ch.receive(buf);
                }
                buf.flip();
                ch.send(buf, from);
            }
        } catch (ClosedChannelException e) {
            // Channel closed at end of run is expected (covers AsynchronousCloseException too).
        } catch (IOException e) {
            // Ignore other IO errors on close.
        }
    }

    private static long[] runClient(DatagramChannel ch, Config cfg) throws Exception {
        int n = cfg.payloadBytes();
        // Allocate once; reused on every round trip (zero alloc on the timed path).
        ByteBuffer sendBuf = ByteBuffer.allocateDirect(n);
        ByteBuffer recvBuf = ByteBuffer.allocateDirect(n);

        for (int i = 0; i < n; i++) {
            sendBuf.put((byte) (i & 0xFF));
        }
        sendBuf.flip(); // position=0, limit=n
        // Read-only view shares content but has independent position/limit; mismatch() won't mutate it.
        ByteBuffer expectedBuf = sendBuf.asReadOnlyBuffer();

        return Measure.run(cfg, () -> roundTrip(ch, sendBuf, recvBuf, expectedBuf, n));
    }

    private static void roundTrip(DatagramChannel ch, ByteBuffer sendBuf, ByteBuffer recvBuf,
            ByteBuffer expectedBuf, int n) throws IOException {
        sendBuf.clear();
        while (sendBuf.hasRemaining()) ch.write(sendBuf);

        recvBuf.clear();
        long deadline = System.nanoTime() + SO_TIMEOUT_MS * 1_000_000L; // computed once per round trip
        while (recvBuf.hasRemaining()) {
            int r = ch.read(recvBuf); // connected channel: 0 when no datagram (non-blocking)
            if (r == 0) {
                if (System.nanoTime() > deadline)
                    throw new IOException("UDP receive timed out after " + SO_TIMEOUT_MS + " ms");
                Thread.onSpinWait();
            }
        }
        recvBuf.flip();
        // mismatch() compares remaining bytes from each buffer's position; positions are unchanged.
        if (recvBuf.mismatch(expectedBuf) >= 0)
            throw new IOException("UDP echo mismatch: received bytes differ from sent");
    }
}
