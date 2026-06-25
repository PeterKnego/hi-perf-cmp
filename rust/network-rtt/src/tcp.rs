//! TCP synchronous ping-pong RTT measurement over loopback.
//!
//! An in-process echo server runs on its own thread; a single client connection
//! issues one request at a time and times each round trip.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Instant;

use crate::config::Config;

/// Run the TCP benchmark and return one elapsed-nanosecond sample per measured
/// round trip (`cfg.iterations` of them).
pub fn run(cfg: &Config) -> io::Result<Vec<u64>> {
    // Bind to an ephemeral loopback port so the OS picks a free one.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let payload_bytes = cfg.payload_bytes;

    // Echo server: accept one connection, echo bytes until the client closes.
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut conn, _) = listener.accept()?;
        conn.set_nodelay(true)?;
        let mut buf = vec![0u8; payload_bytes];
        loop {
            // Read exactly one payload, echo it back. EOF ends the loop.
            match read_exact_or_eof(&mut conn, &mut buf)? {
                false => return Ok(()), // clean EOF
                true => conn.write_all(&buf)?,
            }
        }
    });

    let result = client_loop(addr, cfg);

    // Closing the client stream (dropped inside client_loop) yields EOF on the
    // server side; join surfaces any server-side IO error.
    server
        .join()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "tcp echo server thread panicked"))??;

    result
}

/// Connect, warm up, then measure `cfg.iterations` round trips.
fn client_loop(addr: std::net::SocketAddr, cfg: &Config) -> io::Result<Vec<u64>> {
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

/// Fill `buf` from `conn`; returns `Ok(true)` if a full buffer was read,
/// `Ok(false)` on clean EOF before any bytes, `Err` on short EOF or IO error.
fn read_exact_or_eof(conn: &mut TcpStream, buf: &mut [u8]) -> io::Result<bool> {
    let mut read = 0;
    while read < buf.len() {
        match conn.read(&mut buf[read..]) {
            Ok(0) => {
                if read == 0 {
                    return Ok(false); // clean EOF at a payload boundary
                }
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "tcp echo server: connection closed mid-payload",
                ));
            }
            Ok(n) => read += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}
