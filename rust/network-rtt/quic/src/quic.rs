//! QUIC synchronous ping-pong RTT measurement (quinn).
//!
//! [`serve`] is an echo responder: it accepts connections and, on each accepted
//! bidirectional stream, streams every received byte straight back until the
//! peer closes. [`client`] opens ONE long-lived bidirectional stream and issues
//! strict ping-pong round trips — write the payload, read the full echo back,
//! one outstanding at a time — mirroring the TCP methodology.
//!
//! TLS: the server uses an in-memory self-signed cert (rcgen); the client skips
//! verification (insecure — latency benchmark). ALPN is fixed to `hperf-rtt`.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};

use bench_common::config::Config;

/// Fixed ALPN for both ends; distinguishes this benchmark protocol.
const ALPN: &[u8] = b"hperf-rtt";

/// Map any error into an `io::Error` so the round-trip helper has one error
/// type, matching the std-based experiments.
fn io_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

/// Resolve `host:port` to a single socket address.
pub fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    use std::net::ToSocketAddrs;
    (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("could not resolve {host}:{port}: {e}"))?
        .next()
        .ok_or_else(|| format!("no addresses resolved for {host}:{port}"))
}

/// Build a server [`Endpoint`] bound to `bind`, configured with a fresh
/// in-memory self-signed certificate and the `hperf-rtt` ALPN.
pub fn server_endpoint(bind: &str) -> io::Result<Endpoint> {
    let addr: SocketAddr = bind.parse().map_err(io_err)?;

    // Generate a fresh self-signed cert for `localhost` at startup.
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).map_err(io_err)?;
    let key = PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
    let cert_der = CertificateDer::from(cert.cert);

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], PrivateKeyDer::Pkcs8(key))
        .map_err(io_err)?;
    server_crypto.alpn_protocols = vec![ALPN.to_vec()];

    let server_config = ServerConfig::with_crypto(Arc::new(
        QuicServerConfig::try_from(server_crypto).map_err(io_err)?,
    ));

    Endpoint::server(server_config, addr)
}

/// Echo responder: accept connections forever; for each accepted bidirectional
/// stream, stream every received byte straight back until the peer closes.
/// Never returns (the endpoint is closed only by process exit).
pub async fn serve(endpoint: Endpoint) {
    while let Some(incoming) = endpoint.accept().await {
        tokio::spawn(async move {
            let conn = match incoming.await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("network-rtt-quic: connection failed: {e}");
                    return;
                }
            };
            // Each connection may carry multiple bidi streams; serve each on
            // its own task so a long-lived stream's echo loop runs freely.
            loop {
                match conn.accept_bi().await {
                    Ok((send, recv)) => {
                        tokio::spawn(echo_stream(send, recv));
                    }
                    Err(quinn::ConnectionError::ApplicationClosed { .. })
                    | Err(quinn::ConnectionError::ConnectionClosed { .. })
                    | Err(quinn::ConnectionError::LocallyClosed) => return,
                    Err(e) => {
                        eprintln!("network-rtt-quic: accept_bi ended: {e}");
                        return;
                    }
                }
            }
        });
    }
}

/// Stream every received byte straight back on the same bidirectional stream
/// until the peer finishes/closes. Does NOT wait for end-of-stream before
/// echoing, so the client's per-round-trip ping-pong sees each echo promptly.
///
/// A read/write error here means the peer closed the connection (the normal
/// end of a benchmark run), so it terminates the loop quietly rather than
/// surfacing as a diagnostic.
async fn echo_stream(mut send: quinn::SendStream, mut recv: quinn::RecvStream) {
    let mut buf = [0u8; 8192];
    loop {
        match recv.read(&mut buf).await {
            // Peer finished the stream, or the connection went away — done.
            Ok(None) | Err(_) => {
                let _ = send.finish();
                return;
            }
            Ok(Some(n)) => {
                if send.write_all(&buf[..n]).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// Connect to `addr`, open ONE long-lived bidirectional stream, warm up, then
/// time `cfg.iterations` strict ping-pong round trips. Returns one
/// elapsed-nanosecond sample per measured round trip.
pub async fn client(addr: SocketAddr, cfg: &Config) -> io::Result<Vec<u64>> {
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().map_err(io_err)?)?;
    endpoint.set_default_client_config(client_config()?);

    let conn = endpoint
        .connect(addr, "localhost")
        .map_err(io_err)?
        .await
        .map_err(io_err)?;

    // One long-lived bidirectional stream, mirroring TCP's one connection.
    let (mut send, mut recv) = conn.open_bi().await.map_err(io_err)?;

    let payload = vec![0xEFu8; cfg.payload_bytes];
    let mut echo = vec![0u8; cfg.payload_bytes];

    // Warmup — timings discarded.
    for _ in 0..cfg.warmup {
        round_trip(&mut send, &mut recv, &payload, &mut echo).await?;
    }

    // Pre-allocate the sample buffer so allocation never enters the timed path.
    let mut samples = vec![0u64; cfg.iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip(&mut send, &mut recv, &payload, &mut echo).await?;
        *slot = start.elapsed().as_nanos() as u64;
    }

    // Finish the stream and close the connection cleanly so the server's echo
    // task and accept loop unwind.
    let _ = send.finish();
    conn.close(0u32.into(), b"done");
    endpoint.wait_idle().await;

    Ok(samples)
}

/// One ping-pong: write the full payload, read exactly the echo back, assert
/// equality. One request outstanding at a time.
#[inline]
async fn round_trip(
    send: &mut quinn::SendStream,
    recv: &mut quinn::RecvStream,
    payload: &[u8],
    echo: &mut [u8],
) -> io::Result<()> {
    send.write_all(payload).await.map_err(io_err)?;
    recv.read_exact(echo).await.map_err(io_err)?;
    if echo != payload {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "quic echo mismatch: received bytes differ from sent",
        ));
    }
    Ok(())
}

/// Build a quinn [`ClientConfig`] with an insecure cert verifier (skip
/// validation) and the `hperf-rtt` ALPN.
fn client_config() -> io::Result<ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(SkipServerVerification(provider.clone()));

    let mut client_crypto = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(io_err)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![ALPN.to_vec()];

    Ok(ClientConfig::new(Arc::new(
        QuicClientConfig::try_from(client_crypto).map_err(io_err)?,
    )))
}

/// Insecure server-cert verifier: trusts any certificate. Acceptable here — the
/// benchmark measures latency on a loopback/private network, not security.
#[derive(Debug)]
struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            msg,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            msg,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
