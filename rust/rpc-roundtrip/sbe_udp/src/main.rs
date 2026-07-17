//! rpc-roundtrip sbe_udp cell: UDP transport + zero-copy SBE codec.
//!
//! The responder deserializes each datagram's `hop`, re-serializes `hop + 1`
//! in place, and bounces it back. The client encodes one pre-built payload per
//! iteration, sends it, receives the reply, and verifies `hop == sent + 1`,
//! `seq == sent`. A read timeout is a hard error, never a retransmit.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use bench_common::config::Mode;
use bench_common::measure;
use rpc_roundtrip_common::{RpcConfig, build};
use rpc_roundtrip_sbe_udp::{ENCODED_LEN, encode, mutate_hop_in_place, read_hop, read_seq};

const EXPERIMENT: &str = "sbe_udp";
const FOCUS: &str = "rpc-roundtrip";
const SPIN_BUDGET: u32 = 2048;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

fn prog() -> String {
    format!("{FOCUS}-{EXPERIMENT}")
}

fn main() {
    let cfg = match RpcConfig::from_env() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{}: {msg}", prog());
            std::process::exit(1);
        }
    };
    let result = match cfg.mode {
        Mode::Loopback => run_loopback(&cfg),
        Mode::Server => run_server(&cfg),
        Mode::Client => run_client(&cfg),
    };
    if let Err(e) = result {
        eprintln!("{}: {e}", prog());
        std::process::exit(1);
    }
}

fn run_server(cfg: &RpcConfig) -> io::Result<()> {
    let addr: SocketAddr = format!("0.0.0.0:{}", cfg.udp_port).parse().unwrap();
    eprintln!("{}: serving udp {addr}", prog());
    serve(UdpSocket::bind(addr)?)
}

fn run_loopback(cfg: &RpcConfig) -> io::Result<()> {
    let server = UdpSocket::bind("127.0.0.1:0")?;
    let server_addr = server.local_addr()?;
    std::thread::spawn(move || {
        let _ = serve(server);
    });
    let samples = measure_client(server_addr, cfg)?;
    emit(&samples);
    Ok(())
}

fn run_client(cfg: &RpcConfig) -> io::Result<()> {
    let host = cfg
        .require_host()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let addr: SocketAddr = format!("{host}:{}", cfg.udp_port)
        .parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("{e}")))?;
    let samples = measure_client(addr, cfg)?;
    emit(&samples);
    Ok(())
}

/// Emit the three `rtt_*` lines plus `encoded_bytes` (the four result lines).
fn emit(samples: &[u64]) {
    measure::emit_rtt_with_focus(FOCUS, EXPERIMENT, samples);
    bench_common::result::emit(
        FOCUS,
        EXPERIMENT,
        "encoded_bytes",
        ENCODED_LEN as u64,
        "bytes",
        1,
    );
}

/// Echo responder: mutate `hop` in place and bounce every datagram back.
fn serve(sock: UdpSocket) -> io::Result<()> {
    let mut buf = [0u8; 2048];
    sock.set_nonblocking(true)?;
    loop {
        let (n, src) = {
            let mut spins: u32 = 0;
            loop {
                match sock.recv_from(&mut buf) {
                    Ok(pair) => break pair,
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        spins += 1;
                        if spins >= SPIN_BUDGET {
                            sock.set_nonblocking(false)?;
                            let r = loop {
                                match sock.recv_from(&mut buf) {
                                    Ok(pair) => break Ok(pair),
                                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                                    Err(e) => break Err(e),
                                }
                            };
                            sock.set_nonblocking(true)?;
                            break r?;
                        }
                        std::hint::spin_loop();
                    }
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e),
                }
            }
        };
        mutate_hop_in_place(&mut buf[..n]);
        sock.send_to(&buf[..n], src)?;
    }
}

/// Client: warmup + timed round trips. Each round trip encodes the pre-built
/// request, sends it, receives the reply, and verifies the mutation.
fn measure_client(addr: SocketAddr, cfg: &RpcConfig) -> io::Result<Vec<u64>> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(addr)?;
    sock.set_read_timeout(Some(READ_TIMEOUT))?;

    let req = build(0);
    let mut send_buf = vec![0u8; ENCODED_LEN];
    let n = encode(&req, &mut send_buf);
    let mut recv_buf = [0u8; 2048];

    let round_trip = || -> io::Result<()> {
        sock.send(&send_buf[..n])?;
        let m = sock.recv(&mut recv_buf)?;
        if read_hop(&recv_buf[..m]) != req.hop + 1 || read_seq(&recv_buf[..m]) != req.seq {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "rpc: verification failed",
            ));
        }
        Ok(())
    };

    measure::run_n(cfg.warmup, cfg.iterations, round_trip)
}
