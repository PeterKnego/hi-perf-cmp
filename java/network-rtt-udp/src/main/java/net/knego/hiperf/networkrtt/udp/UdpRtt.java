package net.knego.hiperf.networkrtt.udp;

import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.util.Arrays;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;

/**
 * UDP ping-pong RTT transport. A {@link #serve} responder binds an address and
 * echoes each datagram back to its sender forever; a {@link #client}
 * measurement loop connects one client datagram socket (1s read timeout, where
 * a timeout is a hard error) and, via the shared {@link Measure#run}, times
 * {@code RTT_ITERATIONS} synchronous round trips into a pre-allocated samples
 * buffer. {@link #loopback} wires an in-process server on an ephemeral
 * 127.0.0.1 port to a client.
 */
final class UdpRtt {

    private static final int SO_TIMEOUT_MS = 1000;

    private UdpRtt() {}

    /**
     * Loopback mode: start an in-process echo server on an ephemeral 127.0.0.1
     * port and run the client against it, returning the measured samples.
     */
    static long[] loopback(Config cfg) throws Exception {
        InetAddress loopback = InetAddress.getLoopbackAddress();
        try (DatagramSocket server = new DatagramSocket(new InetSocketAddress(loopback, 0))) {
            int port = server.getLocalPort();

            Thread serverThread = new Thread(() -> runEchoServer(server, cfg.payloadBytes()),
                    "udp-echo-server");
            serverThread.setDaemon(true);
            serverThread.start();

            try {
                return client(loopback, port, cfg);
            } finally {
                server.close(); // unblock the server's receive
                serverThread.join(2000);
            }
        }
    }

    /**
     * Server/responder: bind {@code address:port} and echo each received
     * datagram back to its sender until the process is killed. Logs to stderr.
     */
    static void serve(InetAddress address, int port, int payloadBytes) throws IOException {
        try (DatagramSocket server = new DatagramSocket(new InetSocketAddress(address, port))) {
            System.err.println("network-rtt: UDP responder listening on "
                    + address.getHostAddress() + ":" + port);
            runEchoServer(server, payloadBytes);
        }
    }

    /**
     * Client measurement loop: connect a datagram socket to {@code host:port}
     * with a 1s read timeout (timeout = hard error), run warmup, then time
     * {@code RTT_ITERATIONS} synchronous round trips with an echo-byte equality
     * assertion. Returns the measured samples.
     */
    static long[] client(InetAddress host, int port, Config cfg) throws Exception {
        try (DatagramSocket socket = new DatagramSocket()) {
            socket.connect(host, port);
            socket.setSoTimeout(SO_TIMEOUT_MS);
            return runClient(socket, cfg);
        }
    }

    /** Convenience overload resolving {@code cfg.host()} for client mode. */
    static long[] client(Config cfg) throws Exception {
        InetAddress host = InetAddress.getByName(cfg.host());
        return client(host, cfg.udpPort(), cfg);
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

    private static long[] runClient(DatagramSocket client, Config cfg) throws Exception {
        int n = cfg.payloadBytes();
        byte[] send = new byte[n];
        for (int i = 0; i < n; i++) {
            send[i] = (byte) (i & 0xFF);
        }
        byte[] recvBuf = new byte[n];
        DatagramPacket sendPacket = new DatagramPacket(send, n);
        DatagramPacket recvPacket = new DatagramPacket(recvBuf, n);

        return Measure.run(cfg, () -> roundTrip(client, sendPacket, recvPacket, send, recvBuf, n));
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
