package net.knego.hiperf.common;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.Arrays;

/**
 * Shared durable-append harness for the filesystem-write experiments. Owns file
 * setup (incl. optional preallocation), the warmup + timed loop, and result
 * emission; each experiment supplies (SyncKind, batchSize, prealloc).
 */
public final class DurableAppend {

    /** Focus area shared by all filesystem-write experiments. */
    public static final String FOCUS_AREA = "filesystem-write";

    private DurableAppend() {}

    /** Per-sync latencies (ns) and the end-to-end throughput (entries/sec). */
    public record Outcome(long[] syncSamples, double throughput) {}

    public static Outcome run(FsConfig cfg, String experiment, SyncKind sync, int batchSize, boolean prealloc)
            throws IOException {
        Path path = Path.of(cfg.dir(), "filesystem-write-" + experiment + ".log");
        try (FileChannel ch = FileChannel.open(path,
                StandardOpenOption.CREATE, StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING)) {
            // Make the file's existence durable (file + parent dir), outside timing.
            ch.force(true);
            syncDir(cfg.dir());

            byte[] fill = new byte[cfg.entryBytes()];
            Arrays.fill(fill, (byte) 0xAB);
            ByteBuffer entry = ByteBuffer.allocateDirect(cfg.entryBytes());
            entry.put(fill).flip();

            if (prealloc) {
                preallocate(ch, (long) (cfg.warmup() + cfg.iterations()) * cfg.entryBytes());
            }

            // Warmup (discarded).
            runEntries(ch, entry, cfg.warmup(), batchSize, sync, null);

            int nSyncs = (cfg.iterations() + batchSize - 1) / batchSize;
            long[] samples = new long[nSyncs];
            long tStart = System.nanoTime();
            runEntries(ch, entry, cfg.iterations(), batchSize, sync, samples);
            double throughput = cfg.iterations() / ((System.nanoTime() - tStart) / 1e9);
            return new Outcome(samples, throughput);
        }
    }

    private static void runEntries(FileChannel ch, ByteBuffer entry, int entries, int batchSize,
            SyncKind sync, long[] samples) throws IOException {
        int remaining = entries;
        int idx = 0;
        while (remaining > 0) {
            int count = Math.min(batchSize, remaining);
            for (int i = 0; i < count; i++) {
                entry.rewind();
                while (entry.hasRemaining()) {
                    ch.write(entry);
                }
            }
            long start = System.nanoTime();
            ch.force(sync == SyncKind.FULL); // force(true)=fsync, force(false)=fdatasync
            if (samples != null) {
                samples[idx++] = System.nanoTime() - start;
            }
            remaining -= count;
        }
    }

    private static void preallocate(FileChannel ch, long total) throws IOException {
        // allocateDirect zero-initializes its contents (JLS/Javadoc guarantee),
        // so this is the real zero-write the prealloc experiment requires.
        ByteBuffer zeros = ByteBuffer.allocateDirect(1024 * 1024);
        long written = 0;
        while (written < total) {
            zeros.clear();
            zeros.limit((int) Math.min(zeros.capacity(), total - written));
            while (zeros.hasRemaining()) {
                written += ch.write(zeros);
            }
        }
        ch.force(true);
        ch.position(0);
    }

    private static void syncDir(String dir) {
        try (FileChannel dc = FileChannel.open(Path.of(dir), StandardOpenOption.READ)) {
            dc.force(true);
        } catch (IOException e) {
            // Some platforms disallow opening a directory as a channel; the file
            // fsync above is the primary durability guarantee. Best-effort.
        }
    }

    /** Sort the sync samples and emit the four filesystem-write result lines. */
    public static void emit(String experiment, long[] syncSamples, double throughput, int iterations) {
        long[] sorted = syncSamples.clone();
        Arrays.sort(sorted);
        long nSync = sorted.length;
        new Result(FOCUS_AREA, experiment, "durable_append_throughput", throughput, "ops_per_sec", iterations, "").emit();
        new Result(FOCUS_AREA, experiment, "sync_p50", Stats.percentile(sorted, 50), "ns", nSync, "").emit();
        new Result(FOCUS_AREA, experiment, "sync_p99", Stats.percentile(sorted, 99), "ns", nSync, "").emit();
        new Result(FOCUS_AREA, experiment, "sync_mean", Stats.mean(sorted), "ns", nSync, "").emit();
    }
}
