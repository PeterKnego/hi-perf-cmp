//! UDP synchronous ping-pong RTT measurement.
//!
//! [`serve`] is an echo responder that bounces every datagram back to its
//! sender until the process is killed. [`client`] `connect()`s its socket and
//! issues one datagram at a time via the shared [`bench_common::measure`] loop.
//! A read timeout guards against datagram loss — a timeout is a hard error,
//! never a retransmit.

use std::io::{self};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use bench_common::config::Config;
use bench_common::measure;

/// How many nonblocking `recv_from` misses the responder spins before
/// falling back to a single blocking call so an idle server yields the CPU.
const SPIN_BUDGET: u32 = 2048;

/// Echo responder: bind `addr`, then bounce every datagram back to its sender
/// until the process is killed. Runs forever; never returns `Ok`.
pub fn serve(addr: SocketAddr) -> io::Result<()> {
    serve_socket(UdpSocket::bind(addr)?)
}

/// Echo responder over an already-bound socket (used by loopback mode, which
/// binds an ephemeral port up front to learn its address). Runs forever.
///
/// Uses a bounded nonblocking spin on each receive; after `SPIN_BUDGET`
/// misses falls back to one blocking `recv_from` so an idle server yields
/// the CPU rather than burning it.
pub fn serve_socket(sock: UdpSocket) -> io::Result<()> {
    // Sized to comfortably hold any single benchmark datagram.
    let mut buf = [0u8; 65_535];
    sock.set_nonblocking(true)?;
    loop {
        // Acquire one datagram with a bounded spin, then fall back to blocking.
        let (n, src) = {
            let mut spins: u32 = 0;
            loop {
                match sock.recv_from(&mut buf) {
                    Ok(pair) => break pair,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        spins += 1;
                        if spins >= SPIN_BUDGET {
                            // Spin budget exhausted — switch to blocking to yield the CPU.
                            sock.set_nonblocking(false)?;
                            let result = loop {
                                match sock.recv_from(&mut buf) {
                                    Ok(pair) => break Ok(pair),
                                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                                    Err(e) => break Err(e),
                                }
                            };
                            // Always restore nonblocking before propagating any error.
                            sock.set_nonblocking(true)?;
                            break result?;
                        } else {
                            std::hint::spin_loop();
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        };
        sock.send_to(&buf[..n], src)?;
    }
}

/// Connect to `addr`, warm up, then measure `cfg.iterations` round trips,
/// returning one elapsed-nanosecond sample per measured round trip.
pub fn client(addr: SocketAddr, cfg: &Config) -> io::Result<Vec<u64>> {
    let sock = UdpSocket::bind(bind_addr_for(addr))?;
    sock.connect(addr)?;
    // Nonblocking: busy-poll in round_trip with a ~1s deadline to detect loss.
    sock.set_nonblocking(true)?;

    let send = vec![0xCDu8; cfg.payload_bytes];
    let mut recv = vec![0u8; cfg.payload_bytes];

    measure::run(cfg, move || round_trip(&sock, &send, &mut recv))
}

/// Pick a wildcard bind address in the same family as the responder so the
/// connected socket can reach it (`0.0.0.0:0` for v4, `[::]:0` for v6).
fn bind_addr_for(addr: SocketAddr) -> SocketAddr {
    match addr {
        SocketAddr::V4(_) => SocketAddr::from(([0, 0, 0, 0], 0)),
        SocketAddr::V6(_) => SocketAddr::from(([0u16; 8], 0)),
    }
}

/// One ping-pong datagram: send the payload, recv the echo, assert equality.
#[inline]
fn round_trip(sock: &UdpSocket, send: &[u8], recv: &mut [u8]) -> io::Result<()> {
    sock.send(send)?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut spins: u32 = 0;
    let n = loop {
        match sock.recv(recv) {
            Ok(n) => break n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                // Check the wall-clock deadline only periodically so Instant::now()
                // doesn't dominate the hot spin; still bounds the wait to ~1s.
                spins = spins.wrapping_add(1);
                if spins.is_multiple_of(256) && Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "udp recv timed out (datagram loss)",
                    ));
                }
                std::hint::spin_loop();
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    };
    if &recv[..n] != send {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "udp echo mismatch: received bytes differ from sent",
        ));
    }
    Ok(())
}
