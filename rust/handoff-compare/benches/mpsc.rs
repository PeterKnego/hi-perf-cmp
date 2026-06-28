//! MPSC throughput comparison: our-mp-ring | disruptor | crossbeam | std-mpsc,
//! 2 producers, across burst sizes × pauses. Mirrors disruptor-rs benches/mpsc.rs.

use std::sync::Arc;
use std::sync::atomic::{
    AtomicBool, AtomicI64,
    Ordering::{Acquire, Relaxed, Release},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use criterion::measurement::WallTime;
use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main,
};
use crossbeam::channel::TryRecvError::{Disconnected, Empty};
use crossbeam::channel::TrySendError::Full;
use crossbeam::channel::bounded;
use crossbeam::utils::CachePadded;
use disruptor::{BusySpin, Producer};

const PRODUCERS: usize = 2;
const CAP: usize = 256;
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

fn pin(idx: usize) {
    if let Some(cores) = core_affinity::get_core_ids()
        && !cores.is_empty()
    {
        core_affinity::set_for_current(cores[idx % cores.len()]);
    }
}

/// Persistent producer thread released by a barrier each iteration, so we don't
/// pay thread-spawn cost per sample. (From disruptor-rs benches/mpsc.rs.)
struct BurstProducer {
    start_barrier: Arc<CachePadded<AtomicBool>>,
    stop: Arc<CachePadded<AtomicBool>>,
    join_handle: Option<JoinHandle<()>>,
}

impl BurstProducer {
    fn new<P: 'static + Send + FnMut()>(core: usize, mut produce_one_burst: P) -> Self {
        let start_barrier = Arc::new(CachePadded::new(AtomicBool::new(false)));
        let stop = Arc::new(CachePadded::new(AtomicBool::new(false)));
        let join_handle = {
            let stop = Arc::clone(&stop);
            let start_barrier = Arc::clone(&start_barrier);
            thread::spawn(move || {
                pin(core);
                while !stop.load(Acquire) {
                    while start_barrier
                        .compare_exchange(true, false, Acquire, Relaxed)
                        .is_err()
                    {
                        if stop.load(Acquire) {
                            return;
                        }
                    }
                    produce_one_burst();
                }
            })
        };
        Self {
            start_barrier,
            stop,
            join_handle: Some(join_handle),
        }
    }
    fn start(&self) {
        self.start_barrier.store(true, Release);
    }
    fn stop(&mut self) {
        self.stop.store(true, Release);
        self.join_handle.take().unwrap().join().expect("panic");
    }
}

fn run_benchmark(
    group: &mut BenchmarkGroup<WallTime>,
    id: BenchmarkId,
    burst_size: Arc<AtomicI64>,
    sink: Arc<AtomicI64>,
    params: (i64, u64),
    burst_producers: &[BurstProducer],
) {
    group.bench_with_input(id, &params, move |b, (size, pause_ms)| {
        b.iter_custom(|iters| {
            burst_size.store(*size, Release);
            let count = black_box(*size * burst_producers.len() as i64);
            pause(*pause_ms);
            let start = Instant::now();
            for _ in 0..iters {
                sink.store(0, Release);
                burst_producers.iter().for_each(BurstProducer::start);
                while sink.load(Acquire) != count {}
            }
            start.elapsed()
        })
    });
}

pub fn mpsc_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("mpsc");
    for burst_size in BURST_SIZES {
        group.throughput(Throughput::Elements(burst_size));
        base(&mut group, burst_size as i64);
        for pause_ms in PAUSES_MS {
            let params = (burst_size as i64, pause_ms);
            let desc = format!("burst: {}, pause: {} ms", burst_size, pause_ms);
            our_mp_ring(&mut group, params, &desc);
            disruptor(&mut group, params, &desc);
            crossbeam(&mut group, params, &desc);
            std_mpsc(&mut group, params, &desc);
        }
    }
    group.finish();
}

fn base(group: &mut BenchmarkGroup<WallTime>, size: i64) {
    let sink = Arc::new(AtomicI64::new(0));
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let sink = Arc::clone(&sink);
            let burst_size = Arc::clone(&burst_size);
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire);
                for _ in 0..n {
                    sink.fetch_add(1, Release);
                }
            })
        })
        .collect::<Vec<_>>();
    run_benchmark(
        group,
        BenchmarkId::new("base", size),
        burst_size,
        sink,
        (size, 0),
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
}

fn our_mp_ring(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let (prod, mut cons) = handoff_compare::mpsc::ring(CAP);
    let consumer = {
        let sink = Arc::clone(&sink);
        let stop = Arc::clone(&stop);
        thread::spawn(move || {
            pin(0);
            while !stop.load(Relaxed) {
                let n = cons.drain(usize::MAX, |v| {
                    black_box(v);
                });
                if n > 0 {
                    sink.fetch_add(n as i64, Release);
                }
            }
        })
    };
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let mut pr = prod.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire) as usize;
                pr.batch_publish(n, |k| black_box(k as u64));
            })
        })
        .collect::<Vec<_>>();
    drop(prod);
    run_benchmark(
        group,
        BenchmarkId::new("our-mp-ring", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
    stop.store(true, Relaxed);
    consumer.join().expect("consumer panicked");
}

fn disruptor(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let processor = {
        let sink = Arc::clone(&sink);
        move |event: &Event, _seq: i64, _eob: bool| {
            black_box(event.data);
            sink.fetch_add(1, Release);
        }
    };
    let producer = disruptor::build_multi_producer(CAP, || Event { data: 0 }, BusySpin)
        .handle_events_with(processor)
        .build();
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let mut producer = producer.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire) as usize;
                producer.batch_publish(n, |iter| {
                    for (i, e) in iter.enumerate() {
                        e.data = black_box(i as i64);
                    }
                });
            })
        })
        .collect::<Vec<_>>();
    drop(producer);
    run_benchmark(
        group,
        BenchmarkId::new("disruptor", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
}

fn crossbeam(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = bounded::<Event>(CAP);
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(0);
            loop {
                match r.try_recv() {
                    Ok(event) => {
                        black_box(event.data);
                        sink.fetch_add(1, Release);
                    }
                    Err(Empty) => continue,
                    Err(Disconnected) => break,
                }
            }
        })
    };
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let s = s.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire);
                for data in 0..n {
                    let mut event = Event {
                        data: black_box(data),
                    };
                    while let Err(Full(e)) = s.try_send(event) {
                        event = e;
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    drop(s);
    run_benchmark(
        group,
        BenchmarkId::new("crossbeam", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
    receiver.join().expect("receiver panicked");
}

fn std_mpsc(group: &mut BenchmarkGroup<WallTime>, params: (i64, u64), desc: &str) {
    use std::sync::mpsc::channel;
    let sink = Arc::new(AtomicI64::new(0));
    let (s, r) = channel::<Event>();
    let receiver = {
        let sink = Arc::clone(&sink);
        thread::spawn(move || {
            pin(0);
            while let Ok(event) = r.recv() {
                black_box(event.data);
                sink.fetch_add(1, Release);
            }
        })
    };
    let burst_size = Arc::new(AtomicI64::new(0));
    let mut producers = (0..PRODUCERS)
        .map(|p| {
            let burst_size = Arc::clone(&burst_size);
            let s = s.clone();
            BurstProducer::new(p + 1, move || {
                let n = burst_size.load(Acquire);
                for data in 0..n {
                    s.send(Event {
                        data: black_box(data),
                    })
                    .expect("send");
                }
            })
        })
        .collect::<Vec<_>>();
    drop(s);
    run_benchmark(
        group,
        BenchmarkId::new("std-mpsc", desc),
        burst_size,
        sink,
        params,
        &producers,
    );
    producers.iter_mut().for_each(BurstProducer::stop);
    receiver.join().expect("receiver panicked");
}

criterion_group!(mpsc, mpsc_benchmark);
criterion_main!(mpsc);
