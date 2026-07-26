package net.knego.hiperf.smrcollections.livemvcc;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;
import net.knego.hiperf.common.SmrCollections;
import net.knego.hiperf.common.SmrConfig;
import net.knego.hiperf.smrcollections.CowBook;
import net.knego.hiperf.smrcollections.CowRoot;
import net.knego.hiperf.smrcollections.CowSnapshotter;
import net.knego.hiperf.smrcollections.Workload;

/** smr-collections/live_mvcc (Java): writer latency with concurrent CoW serialization. */
public final class Main {
    private static final String EXPERIMENT = "live_mvcc";

    private record CapMsg(CowRoot root, long t0) {}

    public static void main(String[] args) throws InterruptedException {
        try {
            SmrConfig cfg = SmrConfig.fromEnv();
            CowBook book = new CowBook(cfg);
            Workload.SplitMix rng = new Workload.SplitMix(Workload.SEED);
            Workload.Insert ins = new Workload.Insert();
            for (int i = 0; i < cfg.steady(); i++) {
                Workload.nextInsert(rng, i, cfg.levels(), cfg.tick(), cfg.priceMin(), ins);
                book.insert(ins.orderId, ins.price, ins.qty, ins.side);
            }
            int n = cfg.steady();
            Workload.Update up = new Workload.Update();
            for (int i = 0; i < cfg.warmup(); i++) {
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
            }

            int maxBytes = 64 + cfg.cap() * 64 + cfg.levels() * 2 * 32;
            ArrayBlockingQueue<CapMsg> q = new ArrayBlockingQueue<>(1);
            AtomicBoolean busy = new AtomicBoolean(false);
            List<Long> snapDur = new ArrayList<>();
            long[] snapLenBox = new long[1];
            Thread ser = new Thread(() -> {
                CowSnapshotter s = new CowSnapshotter(maxBytes);
                while (true) {
                    CapMsg m;
                    try {
                        m = q.take();
                    } catch (InterruptedException e) {
                        Thread.currentThread().interrupt();
                        return;
                    }
                    if (m.root() == null) {
                        return; // poison pill
                    }
                    snapLenBox[0] = s.encodeRoot(m.root());
                    snapDur.add(System.nanoTime() - m.t0());
                    busy.set(false);
                }
            });
            ser.start();

            long[] writerNs = new long[cfg.liveIters()];
            long skipped = 0;
            for (int k = 0; k < cfg.liveIters(); k++) {
                long t0 = System.nanoTime();
                if (k % cfg.snapEvery() == 0) {
                    if (busy.get()) {
                        skipped++;
                    } else {
                        busy.set(true);
                        q.put(new CapMsg(book.capture(), t0));
                    }
                }
                Workload.nextUpdate(rng, n, up);
                book.update(up.orderId, up.fillQty);
                writerNs[k] = System.nanoTime() - t0;
            }
            q.put(new CapMsg(null, 0));
            ser.join();
            long[] snapNs = snapDur.stream().mapToLong(Long::longValue).toArray();
            SmrCollections.emitLive(EXPERIMENT, writerNs, snapNs, skipped, snapLenBox[0]);
        } catch (IllegalArgumentException e) {
            System.err.println("smr-collections-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
