//! UDP synchronous ping-pong RTT measurement over loopback.
//!
//! An in-process echo server runs on its own thread; the client `connect()`s
//! its socket and issues one datagram at a time, timing each round trip. A read
//! timeout guards against (unexpected) loopback loss — a timeout is a hard
//! error, never a retransmit.

use std::io::{self};
use std::net::UdpSocket;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Config;

/// Run the UDP benchmark and return one elapsed-nanosecond sample per measured
/// round trip (`cfg.iterations` of them).
pub fn run(cfg: &Config) -> io::Result<Vec<u64>> {
    let server_sock = UdpSocket::bind("127.0.0.1:0")?;
    let server_addr = server_sock.local_addr()?;
    let payload_bytes = cfg.payload_bytes;

    // Signal used to stop the echo server once the client is done.
    let (done_tx, done_rx) = mpsc::channel::<()>();

    // Echo server: bounce every datagram back to its sender. A short read
    // timeout lets the loop check the shutdown signal without blocking forever.
    let server = thread::spawn(move || -> io::Result<()> {
        server_sock.set_read_timeout(Some(Duration::from_millis(200)))?;
        let mut buf = vec![0u8; payload_bytes];
        loop {
            match server_sock.recv_from(&mut buf) {
                Ok((n, src)) => {
                    server_sock.send_to(&buf[..n], src)?;
                }
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    // Timed out waiting for a datagram; check for shutdown.
                    if done_rx.try_recv().is_ok() {
                        return Ok(());
                    }
                }
                Err(e) => return Err(e),
            }
        }
    });

    let result = client_loop(server_addr, cfg);

    // Tell the server to stop and wait for it.
    let _ = done_tx.send(());
    server
        .join()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "udp echo server thread panicked"))??;

    result
}

/// Connect, warm up, then measure `cfg.iterations` round trips.
fn client_loop(server_addr: std::net::SocketAddr, cfg: &Config) -> io::Result<Vec<u64>> {
    let sock = UdpSocket::bind("127.0.0.1:0")?;
    sock.connect(server_addr)?;
    // Timeout = hard error (see module docs); 1s per the design.
    sock.set_read_timeout(Some(Duration::from_secs(1)))?;

    let send = vec![0xCDu8; cfg.payload_bytes];
    let mut recv = vec![0u8; cfg.payload_bytes];

    for _ in 0..cfg.warmup {
        round_trip(&sock, &send, &mut recv)?;
    }

    let mut samples = vec![0u64; cfg.iterations];
    for slot in samples.iter_mut() {
        let start = Instant::now();
        round_trip(&sock, &send, &mut recv)?;
        *slot = start.elapsed().as_nanos() as u64;
    }

    Ok(samples)
}

/// One ping-pong datagram: send the payload, recv the echo, assert equality.
#[inline]
fn round_trip(sock: &UdpSocket, send: &[u8], recv: &mut [u8]) -> io::Result<()> {
    sock.send(send)?;
    let n = sock.recv(recv).map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
            io::Error::new(io::ErrorKind::TimedOut, "udp recv timed out (loopback loss)")
        } else {
            e
        }
    })?;
    if &recv[..n] != send {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "udp echo mismatch: received bytes differ from sent",
        ));
    }
    Ok(())
}
