//! SPSC throughput comparison: our-ring | disruptor | crossbeam | std-mpsc,
//! across burst sizes × pauses. Methodology mirrors disruptor-rs benches/spsc.rs.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use criterion::measurement::WallTime;
use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use crossbeam::channel::TryRecvError::{Disconnected, Empty};
use crossbeam::channel::TrySendError::Full;
use crossbeam::channel::bounded;
use disruptor::{BusySpin, Producer};

const CAP: usize = 128;
const BURST_SIZES: [u64; 3] = [1, 10, 100];
const PAUSES_MS: [u64; 3] = [0, 1, 10];

struct Event {
    data: i64,
}

fn pause(millis: u64) {
    if millis > 0 {
        thread::sleep(Duration::from_millis(millis));
    }
}

/// Pin the current thread to core index `idx` (modulo available cores). No-op if
/// affinity is unavailable.
fn pin(idx: usize) {
    if let Some(cores) = core_affinity::get_core_ids()
        && !cores.is_empty()
    {
        core_affinity::set_for_current(cores[idx % cores.len()]);
    }
}

pub fn spsc_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("spsc");
    for burst_size in BURST_SIZES {
        group.throughput(Throughput::Elements(burst_size));
        base(&mut group, burst_size as i64);
        for pause_ms in PAUSES_MS {
            let inputs = (burst_size as i64, pause_ms);
            let param = format!("burst: {}, pause: {} ms", burst_size, pause_ms);
            our_ring(&mut group, inputs, &param);
            disruptor(&mut group, inputs, &param);
            crossbeam(&mut group, inputs, &param);
            std_mpsc(&mut group, inputs, &param);
        }
    }
    group.finish();
}

fn base(group: &mut BenchmarkGroup<WallTime>, burst_size: i64) {
    let sink = Arc::new(AtomicI64::new(0));
    let id = BenchmarkId::new("base", burst_size);
    group.bench_with_input(id, &burst_size, move |b, size| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                for data in 1..=*size {
                    sink.store(black_box(data), Ordering::Release);
                }
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
}

fn our_ring(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (mut prod, mut cons) = handoff_compare::spsc::channel(CAP);
    let consumer = {
        let sink = Arc::clone(&sink);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            pin(1);
            while !stop.load(Ordering::Relaxed) {
                cons.drain(usize::MAX, |v| sink.store(v as i64, Ordering::Release));
            }
        })
    };
    pin(0);
    let id = BenchmarkId::new("our-ring", param);
    group.bench_with_input(id, &inputs, |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                prod.batch_publish(*size as usize, |k| black_box(k as u64 + 1));
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
    stop.store(true, Ordering::Relaxed);
    consumer.join().expect("consumer panicked");
}

fn disruptor(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let processor = {
        let sink = Arc::clone(&sink);
        move |event: &Event, _seq: i64, _eob: bool| {
            sink.store(event.data, Ordering::Release);
        }
    };
    let mut producer = disruptor::build_single_producer(CAP, || Event { data: 0 }, BusySpin)
        .handle_events_with(processor)
        .build();
    let id = BenchmarkId::new("disruptor", param);
    group.bench_with_input(id, &inputs, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                producer.batch_publish(*size as usize, |iter| {
                    for (i, e) in iter.enumerate() {
                        e.data = black_box(i as i64 + 1);
                    }
                });
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
}

fn crossbeam(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = bounded::<Event>(CAP);
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(1);
            loop {
                match r.try_recv() {
                    Ok(event) => sink.store(event.data, Ordering::Release),
                    Err(Empty) => continue,
                    Err(Disconnected) => break,
                }
            }
        })
    };
    pin(0);
    let id = BenchmarkId::new("crossbeam", param);
    group.bench_with_input(id, &inputs, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                for data in 1..=*size {
                    let mut event = Event {
                        data: black_box(data),
                    };
                    while let Err(Full(e)) = s.try_send(event) {
                        event = e;
                    }
                }
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
    receiver.join().expect("receiver panicked");
}

fn std_mpsc(group: &mut BenchmarkGroup<WallTime>, inputs: (i64, u64), param: &str) {
    use std::sync::mpsc::{TrySendError, sync_channel};
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = sync_channel::<Event>(CAP);
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(1);
            while let Ok(event) = r.recv() {
                sink.store(event.data, Ordering::Release);
            }
        })
    };
    pin(0);
    let id = BenchmarkId::new("std-mpsc", param);
    let s_bench = s.clone(); // clone into closure; keep `s` to signal receiver after bench
    group.bench_with_input(id, &inputs, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                for data in 1..=*size {
                    let mut event = Event {
                        data: black_box(data),
                    };
                    while let Err(TrySendError::Full(e)) = s_bench.try_send(event) {
                        event = e;
                    }
                }
                let last = black_box(*size);
                while sink.load(Ordering::Acquire) != last {}
            }
            start.elapsed()
        })
    });
    drop(s); // disconnect: last sender dropped → receiver r.recv() returns Err
    receiver.join().expect("receiver panicked");
}

criterion_group!(spsc, spsc_benchmark);
criterion_main!(spsc);
