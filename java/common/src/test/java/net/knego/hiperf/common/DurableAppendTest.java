package net.knego.hiperf.common;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

class DurableAppendTest {

    @Test
    void batchAndPreallocProduceExpectedSyncCounts(@TempDir Path dir) throws IOException {
        FsConfig cfg = new FsConfig(dir.toString(), 64, 5, 20, 4);

        DurableAppend.Outcome batch = DurableAppend.run(cfg, "test-batch", SyncKind.DATA, 4, false);
        assertEquals(5, batch.syncSamples().length, "20 entries / batch 4 = 5 syncs");
        assertTrue(batch.throughput() > 0);

        FsConfig odd = new FsConfig(dir.toString(), 64, 5, 21, 4);
        DurableAppend.Outcome batch2 = DurableAppend.run(odd, "test-batch2", SyncKind.DATA, 4, false);
        assertEquals(6, batch2.syncSamples().length, "ceil(21/4) = 6 syncs");

        DurableAppend.Outcome pre = DurableAppend.run(cfg, "test-prealloc", SyncKind.DATA, 1, true);
        assertEquals(20, pre.syncSamples().length);
        long min = (long) (cfg.warmup() + cfg.iterations()) * cfg.entryBytes();
        assertTrue(Files.size(dir.resolve("filesystem-write-test-prealloc.log")) >= min,
                "prealloc file must be at least the preallocated size");
    }
}
