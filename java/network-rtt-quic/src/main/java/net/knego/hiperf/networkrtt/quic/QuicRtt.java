package net.knego.hiperf.networkrtt.quic;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.time.Duration;
import java.util.Arrays;
import java.util.concurrent.CountDownLatch;
import net.knego.hiperf.common.Config;
import net.knego.hiperf.common.Measure;
import tech.kwik.core.QuicClientConnection;
import tech.kwik.core.QuicConnection;
import tech.kwik.core.QuicStream;
import tech.kwik.core.StreamClosedException;
import tech.kwik.core.log.Logger;
import tech.kwik.core.log.NullLogger;
import tech.kwik.core.server.ApplicationProtocolConnection;
import tech.kwik.core.server.ApplicationProtocolConnectionFactory;
import tech.kwik.core.server.ServerConnectionConfig;
import tech.kwik.core.server.ServerConnector;

/**
 * QUIC ping-pong RTT transport over a single long-lived bidirectional stream.
 *
 * <p>Methodology mirrors TCP for comparability: one connection, one bidi
 * stream, strict sequential ping-pong (write {@code payload_bytes}, read the
 * full echo back), one outstanding request at a time, warmup discarded, then
 * {@code RTT_ITERATIONS} timed round trips. The server echoes stream bytes back.
 *
 * <p>TLS uses an in-memory self-signed cert on the server; the client skips
 * verification. ALPN is {@code hperf-rtt}. Pure-Java QUIC via Kwik — no native
 * libraries.
 */
final class QuicRtt {

    static final String ALPN = "hperf-rtt";
    private static final Logger LOG = new NullLogger();
    // Generous per-stream receive buffer so a single bidi stream never stalls.
    private static final long STREAM_BUFFER = 1L << 20;
    // Idle timeout: kept short so a stalled connection (see MAX_ATTEMPTS below)
    // surfaces in seconds rather than the QUIC default tens of seconds. The
    // ping-pong is continuous, so this never fires during a healthy run.
    private static final int IDLE_TIMEOUT_SECONDS = 5;
    // Kwik 0.10.10 occasionally wedges its loss-recovery under spurious PTO
    // (scheduling-induced ACK delays > PTO on a busy host), stalling the
    // connection until the idle timeout closes it. That is a transport-library
    // fault, not a latency result, so a measurement attempt that stalls is
    // discarded and re-run on a fresh connection. Each attempt is an independent
    // full clean measurement, so retrying does not bias the reported samples.
    // Echo-byte mismatches are NOT retried — they remain a hard failure.
    private static final int MAX_ATTEMPTS = 5;

    private QuicRtt() {}

    /**
     * Loopback mode: start an in-process QUIC echo server on an ephemeral
     * 127.0.0.1 port and run the client against it, returning the samples.
     */
    static long[] loopback(Config cfg) throws Exception {
        InetAddress loopback = InetAddress.getLoopbackAddress();
        // Bind an ephemeral UDP socket and hand it to the server connector so we
        // can discover the port the client should connect to.
        try (DatagramSocket socket = new DatagramSocket(new InetSocketAddress(loopback, 0))) {
            int port = socket.getLocalPort();
            ServerConnector connector = buildServer(socket, cfg.payloadBytes());
            connector.start();
            System.err.println("network-rtt: QUIC responder (loopback) on "
                    + loopback.getHostAddress() + ":" + port);
            try {
                return client(loopback.getHostAddress(), port, cfg);
            } finally {
                connector.close();
            }
        }
    }

    /**
     * Server/responder: bind {@code address:port} and echo bidi-stream bytes for
     * every accepted connection until the process is killed. Logs to stderr and
     * blocks forever.
     */
    static void serve(InetAddress address, int port, int payloadBytes) throws Exception {
        DatagramSocket socket = new DatagramSocket(new InetSocketAddress(address, port));
        ServerConnector connector = buildServer(socket, payloadBytes);
        connector.start();
        System.err.println("network-rtt: QUIC responder listening on "
                + address.getHostAddress() + ":" + port);
        // Block forever; the process is killed to stop serving.
        new CountDownLatch(1).await();
    }

    /** Convenience overload resolving {@code cfg.host()} for client mode. */
    static long[] client(Config cfg) throws Exception {
        return client(cfg.host(), cfg.quicPort(), cfg);
    }

    /**
     * Client measurement loop: connect to {@code host:port} (ALPN hperf-rtt, no
     * cert check), open one bidirectional stream, run warmup, then time
     * {@code RTT_ITERATIONS} synchronous round trips with an echo-byte equality
     * assertion. Returns the measured samples.
     */
    static long[] client(String host, int port, Config cfg) throws Exception {
        StreamClosedException lastStall = null;
        for (int attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {
            QuicClientConnection connection = buildClient(host, port);
            connection.connect();
            try {
                // One long-lived client-initiated bidirectional stream.
                QuicStream stream = connection.createStream(true);
                InputStream in = stream.getInputStream();
                OutputStream out = stream.getOutputStream();
                int n = cfg.payloadBytes();

                byte[] send = new byte[n];
                for (int i = 0; i < n; i++) {
                    send[i] = (byte) (i & 0xFF);
                }
                byte[] recv = new byte[n];

                return Measure.run(cfg, () -> roundTrip(in, out, send, recv, n));
            } catch (StreamClosedException e) {
                // Transport stalled mid-measurement (Kwik loss-recovery wedge).
                // Discard this partial attempt and re-measure on a fresh
                // connection. Echo mismatches are a plain IOException and are
                // NOT caught here, so they remain a hard failure.
                lastStall = e;
                System.err.println("network-rtt-quic: connection stalled (Kwik loss-recovery);"
                        + " retrying measurement " + attempt + "/" + MAX_ATTEMPTS + ": "
                        + e.getMessage());
            } finally {
                connection.close();
            }
        }
        throw new IOException("QUIC measurement failed after " + MAX_ATTEMPTS
                + " attempts: connection stalled each time (Kwik loss-recovery)", lastStall);
    }

    /**
     * Build a QUIC client connection (ALPN hperf-rtt, skip cert verification),
     * keeping Kwik's insecure-config warning off stdout.
     */
    private static QuicClientConnection buildClient(String host, int port) throws Exception {
        java.io.PrintStream realOut = System.out;
        System.setOut(System.err);
        try {
            return QuicClientConnection.newBuilder()
                    .uri(java.net.URI.create("https://" + host + ":" + port))
                    .applicationProtocol(ALPN)
                    .noServerCertificateCheck()
                    .maxIdleTimeout(Duration.ofSeconds(IDLE_TIMEOUT_SECONDS))
                    .defaultStreamReceiveBufferSize(STREAM_BUFFER)
                    .connectTimeout(Duration.ofSeconds(10))
                    .logger(LOG)
                    .build();
        } finally {
            System.setOut(realOut);
        }
    }

    private static void roundTrip(InputStream in, OutputStream out, byte[] send, byte[] recv, int n)
            throws IOException {
        out.write(send, 0, n);
        out.flush();
        if (!readFully(in, recv, n)) {
            // Stream/connection closed under us — retryable (see client()).
            throw new StreamClosedException("QUIC echo server closed stream mid-round-trip");
        }
        if (!Arrays.equals(send, recv)) {
            // Correctness violation — NOT retryable.
            throw new IOException("QUIC echo mismatch: received bytes differ from sent");
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

    /** Build (but do not start) a QUIC server connector bound to {@code socket}. */
    private static ServerConnector buildServer(DatagramSocket socket, int payloadBytes)
            throws Exception {
        ServerConnectionConfig config = ServerConnectionConfig.builder()
                .maxIdleTimeoutInSeconds(IDLE_TIMEOUT_SECONDS)
                .maxOpenPeerInitiatedBidirectionalStreams(10)
                .maxBidirectionalStreamBufferSize(STREAM_BUFFER)
                .maxConnectionBufferSize(STREAM_BUFFER * 2)
                .build();

        ServerConnector connector = ServerConnector.builder()
                .withSocket(socket)
                .withKeyStore(SelfSignedCert.generate(), SelfSignedCert.ALIAS,
                        SelfSignedCert.PASSWORD)
                .withSupportedVersion(QuicConnection.QuicVersion.V1)
                .withConfiguration(config)
                .withLogger(LOG)
                .build();
        connector.registerApplicationProtocol(ALPN, new EchoFactory(payloadBytes));
        return connector;
    }

    /** Factory that wires every new QUIC connection to an echoing handler. */
    private static final class EchoFactory implements ApplicationProtocolConnectionFactory {
        private final int payloadBytes;

        EchoFactory(int payloadBytes) {
            this.payloadBytes = payloadBytes;
        }

        @Override
        public int maxConcurrentPeerInitiatedBidirectionalStreams() {
            return 10;
        }

        @Override
        public ApplicationProtocolConnection createConnection(String protocol,
                QuicConnection connection) {
            return new EchoConnection(payloadBytes);
        }
    }

    /** Echoes every peer-initiated bidirectional stream, byte-for-byte. */
    private static final class EchoConnection implements ApplicationProtocolConnection {
        private final int payloadBytes;

        EchoConnection(int payloadBytes) {
            this.payloadBytes = payloadBytes;
        }

        @Override
        public void acceptPeerInitiatedStream(QuicStream stream) {
            Thread t = new Thread(() -> echo(stream), "quic-echo-stream");
            t.setDaemon(true);
            t.start();
        }

        private void echo(QuicStream stream) {
            try {
                InputStream in = stream.getInputStream();
                OutputStream out = stream.getOutputStream();
                byte[] buf = new byte[payloadBytes];
                while (true) {
                    if (!readFully(in, buf, payloadBytes)) {
                        return; // client closed the stream
                    }
                    out.write(buf, 0, payloadBytes);
                    out.flush();
                }
            } catch (IOException e) {
                // Stream closed / connection reset at end of run is expected.
            }
        }
    }
}
