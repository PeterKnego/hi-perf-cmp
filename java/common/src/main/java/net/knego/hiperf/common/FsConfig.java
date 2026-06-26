package net.knego.hiperf.common;

/**
 * filesystem-write configuration from the {@code FSW_*} env vars. {@code FSW_DIR}
 * is required (no default) to avoid silently benchmarking a tmpfs; numeric values
 * must be positive integers.
 */
public record FsConfig(String dir, int entryBytes, int warmup, int iterations, int batch) {

    public static FsConfig fromEnv() {
        String dir = Env.trimmedOrNull(System.getenv("FSW_DIR"));
        if (dir == null) {
            throw new IllegalArgumentException(
                    "FSW_DIR is required (set FSW_DIR=<dir on a real disk, not tmpfs>)");
        }
        return new FsConfig(
                dir,
                Env.readPositiveInt("FSW_ENTRY_BYTES", 256),
                Env.readPositiveInt("FSW_WARMUP", 5000),
                Env.readPositiveInt("FSW_ITERATIONS", 50000),
                Env.readPositiveInt("FSW_BATCH", 32));
    }
}
