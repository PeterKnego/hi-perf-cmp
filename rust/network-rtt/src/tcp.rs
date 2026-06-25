//! TCP synchronous ping-pong RTT measurement.
//!
//! [`serve`] is an echo responder that accepts connections and bounces back
//! every payload until the process is killed. [`client`] connects to a
//! responder, warms up, then times one request/response round trip at a time.
//! `loopback` mode wires an in-process [`serve`] to a [`client`]; the
//! `server`/`client` modes run one half across hosts.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use crate::config::Config;

/// Echo responder: bind `addr`, then accept connections and echo every payload
/// back until the process is killed. Each accepted connection is served on its
/// own thread with `TCP_NODELAY` enabled. Runs forever; never returns `Ok`.
pub fn serve(addr: SocketAddr) -> io::Result<()> {
    serve_listener(TcpListener::bind(addr)?)
}

/// Echo responder over an already-bound listener (used by loopback mode, which
/// binds an ephemeral port up front to learn its address). Runs forever.
pub fn serve_listener(listener: TcpListener) -> io::Result<()> {
    loop {
        let (conn, _) = listener.accept()?;
        // Serve each connection independently so one client closing does not
        // stop the responder. Errors on a single connection are logged, not
        // fatal to the server.
        std::thread::spawn(move || {
            if let Err(e) = serve_conn(conn) {
                eprintln!("network-rtt: tcp connection ended: {e}");
            }
        });
    }
}

/// Echo every payload on a single accepted connection until the peer closes.
fn serve_conn(mut conn: TcpStream) -> io::Result<()> {
    conn.set_nodelay(true)?;
    // The responder echoes whatever it reads, so it does not need to know the
    // payload size up front; a fixed buffer streams bytes straight back.
    let mut buf = [0u8; 8192];
    loop {
        match conn.read(&mut buf) {
            Ok(0) => return Ok(()), // clean EOF
            Ok(n) => conn.write_all(&buf[..n])?,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Connect to `addr`, warm up, then measure `cfg.iterations` round trips,
/// returning one elapsed-nanosecond sample per measured round trip.
pub fn client(addr: SocketAddr, cfg: &Config) -> io::Result<Vec<u64>> {
    use std::time::Instant;

    let mut stream = TcpStream::connect(addr)?;
    stream.set_nodelay(true)?;

    let send = vec![0xABu8; cfg.payload_bytes];
    let mut recv = vec![0u8; cfg.payload_bytes];

    // Warmup — timings discarded.
    for _ in 0..cfg.warmup {
        round_trip(&mut stream, &send, &mut recv)?;
    }

    // Pre-allocate the sample buffer so allocation never enters the timed path.
    let mut samples = vec![0u64; cfg.iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip(&mut stream, &send, &mut recv)?;
        *slot = start.elapsed().as_nanos() as u64;
    }

    Ok(samples)
}

/// One ping-pong: write the full payload, read the full echo, assert equality.
#[inline]
fn round_trip(stream: &mut TcpStream, send: &[u8], recv: &mut [u8]) -> io::Result<()> {
    stream.write_all(send)?;
    stream.read_exact(recv)?;
    if recv != send {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "tcp echo mismatch: received bytes differ from sent",
        ));
    }
    Ok(())
}
