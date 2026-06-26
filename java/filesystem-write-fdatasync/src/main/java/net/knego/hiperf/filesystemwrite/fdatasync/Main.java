package net.knego.hiperf.filesystemwrite.fdatasync;

import net.knego.hiperf.common.DurableAppend;
import net.knego.hiperf.common.FsConfig;
import net.knego.hiperf.common.SyncKind;

/**
 * filesystem-write / fdatasync experiment (Java): append one entry, fdatasync per
 * entry. Emits four result-contract JSON lines. See docs/result-contract.md.
 */
public final class Main {

    private static final String EXPERIMENT = "fdatasync";

    public static void main(String[] args) {
        try {
            FsConfig cfg = FsConfig.fromEnv();
            DurableAppend.Outcome out = DurableAppend.run(cfg, EXPERIMENT, SyncKind.DATA, 1, false);
            DurableAppend.emit(EXPERIMENT, out.syncSamples(), out.throughput(), cfg.iterations());
        } catch (IllegalArgumentException e) {
            System.err.println("filesystem-write-" + EXPERIMENT + ": invalid configuration: " + e.getMessage());
            System.exit(1);
        } catch (Exception e) {
            System.err.println("filesystem-write-" + EXPERIMENT + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
