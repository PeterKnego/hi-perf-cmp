//! UDP synchronous ping-pong RTT measurement.
//!
//! [`serve`] is an echo responder that bounces every datagram back to its
//! sender until the process is killed. [`client`] `connect()`s its socket and
//! issues one datagram at a time via the shared [`bench_common::measure`] loop.
//! A read timeout guards against datagram loss — a timeout is a hard error,
//! never a retransmit.

use std::io::{self};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use bench_common::config::Config;
use bench_common::measure;

/// Echo responder: bind `addr`, then bounce every datagram back to its sender
/// until the process is killed. Runs forever; never returns `Ok`.
pub fn serve(addr: SocketAddr) -> io::Result<()> {
    serve_socket(UdpSocket::bind(addr)?)
}

/// Echo responder over an already-bound socket (used by loopback mode, which
/// binds an ephemeral port up front to learn its address). Runs forever.
pub fn serve_socket(sock: UdpSocket) -> io::Result<()> {
    // Sized to comfortably hold any single benchmark datagram.
    let mut buf = [0u8; 65_535];
    loop {
        match sock.recv_from(&mut buf) {
            Ok((n, src)) => {
                sock.send_to(&buf[..n], src)?;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Connect to `addr`, warm up, then measure `cfg.iterations` round trips,
/// returning one elapsed-nanosecond sample per measured round trip.
pub fn client(addr: SocketAddr, cfg: &Config) -> io::Result<Vec<u64>> {
    let sock = UdpSocket::bind(bind_addr_for(addr))?;
    sock.connect(addr)?;
    // Timeout = hard error (see module docs); 1s per the design.
    sock.set_read_timeout(Some(Duration::from_secs(1)))?;

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
    let n = sock.recv(recv).map_err(|e| {
        if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "udp recv timed out (datagram loss)",
            )
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
