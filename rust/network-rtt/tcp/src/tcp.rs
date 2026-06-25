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

/// Number of nonblocking poll attempts before the responder falls back to a
/// blocking read. Keeps the hot path spin-tight while bounding CPU waste when
/// the next request is delayed (contention, slow client, etc.).
const SPIN_BUDGET: u32 = 4000;

/// One bounded-spin-then-blocking read attempt.
///
/// Polls the (nonblocking) stream up to `budget` times on WouldBlock/Interrupted,
/// then falls back to a single blocking read (restoring nonblocking mode
/// afterwards). Returns the number of bytes read; `Ok(0)` means clean EOF.
/// Does NOT loop to fill a full buffer — returns as soon as any bytes arrive.
fn read_bounded(stream: &mut TcpStream, buf: &mut [u8], budget: u32) -> io::Result<usize> {
    let mut tries = 0u32;
    loop {
        match stream.read(buf) {
            Ok(n) => return Ok(n), // includes Ok(0) = EOF
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock
                    || e.kind() == io::ErrorKind::Interrupted =>
            {
                if tries < budget {
                    tries += 1;
                    std::hint::spin_loop();
                } else {
                    // Budget exhausted: switch to blocking to yield CPU.
                    stream.set_nonblocking(false)?;
                    let r = loop {
                        match stream.read(buf) {
                            Ok(n) => break Ok(n),
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                            Err(e) => break Err(e),
                        }
                    };
                    // Always restore nonblocking mode before returning.
                    stream.set_nonblocking(true)?;
                    return r;
                }
            }
            Err(e) => return Err(e),
        }
    }
}

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
    conn.set_nonblocking(true)?;
    // The responder echoes whatever it reads, so it does not need to know the
    // payload size up front; a fixed buffer streams bytes straight back.
    let mut buf = [0u8; 8192];
    loop {
        // Bounded-spin read: poll nonblocking up to SPIN_BUDGET times, then
        // fall back to blocking so the thread yields under contention.
        let n = match read_bounded(&mut conn, &mut buf, SPIN_BUDGET)? {
            0 => return Ok(()), // clean EOF
            n => n,
        };

        // Nonblocking write: spin until all bytes are echoed.
        let mut off = 0;
        while off < n {
            match conn.write(&buf[off..n]) {
                Ok(m) => off += m,
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::Interrupted =>
                {
                    std::hint::spin_loop();
                }
                Err(e) => return Err(e),
            }
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

    // Read: bounded-spin-then-block until the echo buffer is full.
    let mut filled = 0usize;
    while filled < recv.len() {
        let n = read_bounded(stream, &mut recv[filled..], SPIN_BUDGET)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "tcp echo: peer closed connection",
            ));
        }
        filled += n;
    }

    if recv != send {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "tcp echo mismatch: received bytes differ from sent",
        ));
    }
    Ok(())
}
