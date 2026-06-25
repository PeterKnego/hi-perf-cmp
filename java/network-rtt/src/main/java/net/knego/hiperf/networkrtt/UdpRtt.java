package net.knego.hiperf.networkrtt;

import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.util.Arrays;

/**
 * UDP ping-pong RTT measurement over loopback. Starts an in-process echo
 * server on its own thread, connects one client datagram socket to it, and
 * times {@code RTT_ITERATIONS} synchronous round trips. A receive timeout is
 * treated as a hard error (loopback UDP is effectively lossless).
 */
final class UdpRtt {

    private static final int SO_TIMEOUT_MS = 1000;

    private UdpRtt() {}

    static long[] measure(Config cfg) throws IOException, InterruptedException {
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (DatagramSocket server = new DatagramSocket(new InetSocketAddress(loopback, 0))) {
            int port = server.getLocalPort();

            Thread serverThread = new Thread(() -> runEchoServer(server, cfg.payloadBytes()),
                    "udp-echo-server");
            serverThread.setDaemon(true);
            serverThread.start();

            try (DatagramSocket client = new DatagramSocket()) {
                client.connect(loopback, port);
                client.setSoTimeout(SO_TIMEOUT_MS);
                return runClient(client, cfg);
            } finally {
                server.close(); // unblock the server's receive
                serverThread.join(2000);
            }
        }
    }

    /** Echoes each received datagram back to its sender until the socket is closed. */
    private static void runEchoServer(DatagramSocket server, int payloadBytes) {
        byte[] buf = new byte[payloadBytes];
        DatagramPacket packet = new DatagramPacket(buf, buf.length);
        try {
            while (true) {
                packet.setLength(buf.length);
                server.receive(packet);
                server.send(new DatagramPacket(packet.getData(), packet.getLength(),
                        packet.getAddress(), packet.getPort()));
            }
        } catch (IOException e) {
            // Socket closed at end of run is expected.
        }
    }

    private static long[] runClient(DatagramSocket client, Config cfg) throws IOException {
        int n = cfg.payloadBytes();
        byte[] send = new byte[n];
        for (int i = 0; i < n; i++) {
            send[i] = (byte) (i & 0xFF);
        }
        byte[] recvBuf = new byte[n];
        DatagramPacket sendPacket = new DatagramPacket(send, n);
        DatagramPacket recvPacket = new DatagramPacket(recvBuf, n);

        for (int i = 0; i < cfg.warmup(); i++) {
            roundTrip(client, sendPacket, recvPacket, send, recvBuf, n);
        }

        long[] samples = new long[cfg.iterations()]; // pre-allocated; no alloc in timed path
        for (int i = 0; i < cfg.iterations(); i++) {
            long start = System.nanoTime();
            roundTrip(client, sendPacket, recvPacket, send, recvBuf, n);
            samples[i] = System.nanoTime() - start;
        }
        return samples;
    }

    private static void roundTrip(DatagramSocket client, DatagramPacket sendPacket,
            DatagramPacket recvPacket, byte[] send, byte[] recvBuf, int n) throws IOException {
        client.send(sendPacket);
        recvPacket.setLength(n);
        try {
            client.receive(recvPacket);
        } catch (SocketTimeoutException e) {
            throw new IOException("UDP receive timed out after " + SO_TIMEOUT_MS + " ms", e);
        }
        if (recvPacket.getLength() != n
                || !Arrays.equals(send, 0, n, recvBuf, 0, recvPacket.getLength())) {
            throw new IOException("UDP echo mismatch: received bytes differ from sent");
        }
    }
}
