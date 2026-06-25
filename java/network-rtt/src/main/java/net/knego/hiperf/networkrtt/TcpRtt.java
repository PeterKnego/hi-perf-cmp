package net.knego.hiperf.networkrtt;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.util.Arrays;

/**
 * TCP ping-pong RTT measurement over loopback. Starts an in-process echo
 * server on its own thread, opens one client connection with Nagle disabled,
 * and times {@code RTT_ITERATIONS} synchronous round trips.
 */
final class TcpRtt {

    private TcpRtt() {}

    static long[] measure(Config cfg) throws IOException, InterruptedException {
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (ServerSocket server = new ServerSocket()) {
            server.bind(new InetSocketAddress(loopback, 0));
            int port = server.getLocalPort();

            Thread serverThread = new Thread(() -> runEchoServer(server, cfg.payloadBytes()),
                    "tcp-echo-server");
            serverThread.setDaemon(true);
            serverThread.start();

            try (Socket client = new Socket()) {
                client.connect(new InetSocketAddress(loopback, port));
                client.setTcpNoDelay(true);

                long[] samples = runClient(client, cfg);
                serverThread.join(2000);
                return samples;
            }
        }
    }

    /** Echoes a fixed-size payload back for every round trip until the client closes. */
    private static void runEchoServer(ServerSocket server, int payloadBytes) {
        try (Socket conn = server.accept()) {
            conn.setTcpNoDelay(true);
            InputStream in = conn.getInputStream();
            OutputStream out = conn.getOutputStream();
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

    private static long[] runClient(Socket client, Config cfg) throws IOException {
        InputStream in = client.getInputStream();
        OutputStream out = client.getOutputStream();
        int n = cfg.payloadBytes();

        byte[] send = new byte[n];
        for (int i = 0; i < n; i++) {
            send[i] = (byte) (i & 0xFF);
        }
        byte[] recv = new byte[n];

        // Warmup — discarded.
        for (int i = 0; i < cfg.warmup(); i++) {
            roundTrip(in, out, send, recv, n);
        }

        long[] samples = new long[cfg.iterations()]; // pre-allocated; no alloc in timed path
        for (int i = 0; i < cfg.iterations(); i++) {
            long start = System.nanoTime();
            roundTrip(in, out, send, recv, n);
            samples[i] = System.nanoTime() - start;
        }
        return samples;
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
