//! TCP synchronous ping-pong RTT measurement.
//!
//! [`serve`] is an echo responder that accepts connections and bounces back
//! every payload until the process is killed. [`client`] connects to a
//! responder, warms up, then times one request/response round trip at a time
//! via the shared [`bench_common::measure`] loop. `loopback` mode wires an
//! in-process [`serve`] to a [`client`]; the `server`/`client` modes run one
//! half across hosts.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use bench_common::config::Config;
use bench_common::measure;

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
                eprintln!("network-rtt-tcp: connection ended: {e}");
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
    let mut stream = TcpStream::connect(addr)?;
    stream.set_nodelay(true)?;
    // Busy-poll the socket to avoid kernel park/unpark on each round trip.
    stream.set_nonblocking(true)?;

    let send = vec![0xABu8; cfg.payload_bytes];
    // The closure owns the stream and recv buffer mutably; `measure::run` calls
    // it once per round trip. Both are allocated before the timed loop.
    let mut recv = vec![0u8; cfg.payload_bytes];

    measure::run(cfg, move || round_trip(&mut stream, &send, &mut recv))
}

/// One ping-pong: write the full payload, read the full echo, assert equality.
/// The socket is nonblocking; spin on WouldBlock/Interrupted instead of sleeping.
#[inline]
fn round_trip(stream: &mut TcpStream, send: &[u8], recv: &mut [u8]) -> io::Result<()> {
    // Write: advance offset until the full payload is sent.
    let mut off = 0;
    while off < send.len() {
        match stream.write(&send[off..]) {
            Ok(n) => off += n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock
                   || e.kind() == io::ErrorKind::Interrupted => {
                std::hint::spin_loop();
            }
            Err(e) => return Err(e),
        }
    }

    // Read: busy-poll until the echo buffer is full.
    let mut filled = 0;
    while filled < recv.len() {
        match stream.read(&mut recv[filled..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "tcp echo: peer closed connection",
                ));
            }
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock
                   || e.kind() == io::ErrorKind::Interrupted => {
                std::hint::spin_loop();
            }
            Err(e) => return Err(e),
        }
    }

    if recv != send {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "tcp echo mismatch: received bytes differ from sent",
        ));
    }
    Ok(())
}
