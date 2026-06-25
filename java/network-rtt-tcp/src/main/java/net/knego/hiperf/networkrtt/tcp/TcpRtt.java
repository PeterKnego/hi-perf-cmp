package net.knego.hiperf.networkrtt.tcp;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.Arrays;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;

/**
 * TCP ping-pong RTT transport. A {@link #serve} responder binds an address and
 * echoes fixed-size payloads back forever; a {@link #client} measurement loop
 * opens one connection with Nagle disabled and, via the shared
 * {@link Measure#run}, times {@code RTT_ITERATIONS} synchronous round trips into
 * a pre-allocated samples buffer. {@link #loopback} wires an in-process server
 * on an ephemeral 127.0.0.1 port to a client.
 */
final class TcpRtt {

    private TcpRtt() {}

    /**
     * Loopback mode: start an in-process echo server on an ephemeral 127.0.0.1
     * port and run the client against it, returning the measured samples.
     */
    static long[] loopback(Config cfg) throws Exception {
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (ServerSocket server = new ServerSocket()) {
            server.bind(new InetSocketAddress(loopback, 0));
            int port = server.getLocalPort();

            Thread serverThread = new Thread(() -> runEchoServer(server, cfg.payloadBytes()),
                    "tcp-echo-server");
            serverThread.setDaemon(true);
            serverThread.start();

            try {
                long[] samples = client(loopback, port, cfg);
                serverThread.join(2000);
                return samples;
            } finally {
                server.close();
            }
        }
    }

    /**
     * Server/responder: bind {@code address:port} and echo fixed-size payloads
     * for every accepted connection until the process is killed. Each
     * connection is served on its own daemon thread. Logs to stderr.
     */
    static void serve(InetAddress address, int port, int payloadBytes) throws IOException {
        try (ServerSocket server = new ServerSocket()) {
            server.bind(new InetSocketAddress(address, port));
            System.err.println("network-rtt: TCP responder listening on "
                    + address.getHostAddress() + ":" + port);
            while (true) {
                Socket conn = server.accept();
                Thread t = new Thread(() -> serveConnection(conn, payloadBytes),
                        "tcp-echo-conn");
                t.setDaemon(true);
                t.start();
            }
        }
    }

    /**
     * Client measurement loop: connect to {@code host:port} (Nagle disabled),
     * run warmup, then time {@code RTT_ITERATIONS} synchronous round trips with
     * an echo-byte equality assertion. Returns the measured samples.
     */
    static long[] client(InetAddress host, int port, Config cfg) throws Exception {
        try (Socket socket = new Socket()) {
            socket.connect(new InetSocketAddress(host, port));
            socket.setTcpNoDelay(true);
            return runClient(socket, cfg);
        }
    }

    /** Convenience overload resolving {@code cfg.host()} for client mode. */
    static long[] client(Config cfg) throws Exception {
        InetAddress host = InetAddress.getByName(cfg.host());
        return client(host, cfg.tcpPort(), cfg);
    }

    /** Echoes a fixed-size payload back for every round trip until the client closes. */
    private static void serveConnection(Socket conn, int payloadBytes) {
        try (Socket c = conn) {
            c.setTcpNoDelay(true);
            InputStream in = c.getInputStream();
            OutputStream out = c.getOutputStream();
            byte[] buf = new byte[payloadBytes];
            while (true) {
                if (!readFully(in, buf, payloadBytes)) {
                    return; // client closed
                }
                out.write(buf, 0, payloadBytes);
                out.flush();
            }
        } catch (IOException e) {
            // Connection closed / reset on client shutdown is expected at end of run.
        }
    }

    /** Echoes a single accepted connection (used by the in-process loopback server). */
    private static void runEchoServer(ServerSocket server, int payloadBytes) {
        try {
            serveConnection(server.accept(), payloadBytes);
        } catch (IOException e) {
            // Server socket closed at end of run is expected.
        }
    }

    private static long[] runClient(Socket client, Config cfg) throws Exception {
        InputStream in = client.getInputStream();
        OutputStream out = client.getOutputStream();
        int n = cfg.payloadBytes();

        byte[] send = new byte[n];
        for (int i = 0; i < n; i++) {
            send[i] = (byte) (i & 0xFF);
        }
        byte[] recv = new byte[n];

        return Measure.run(cfg, () -> roundTrip(in, out, send, recv, n));
    }

    private static void roundTrip(InputStream in, OutputStream out, byte[] send, byte[] recv, int n)
            throws IOException {
        out.write(send, 0, n);
        out.flush();
        if (!readFully(in, recv, n)) {
            throw new IOException("TCP echo server closed mid-round-trip");
        }
        if (!Arrays.equals(send, recv)) {
            throw new IOException("TCP echo mismatch: received bytes differ from sent");
        }
    }

    private static boolean readFully(InputStream in, byte[] buf, int len) throws IOException {
        int off = 0;
        while (off < len) {
            int r = in.read(buf, off, len - off);
            if (r < 0) {
                return false;
            }
            off += r;
        }
        return true;
    }
}
