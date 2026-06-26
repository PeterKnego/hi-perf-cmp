package net.knego.hiperf.common;

/** Which durability barrier to issue per commit. */
public enum SyncKind {
    /** Full fsync via {@code FileChannel.force(true)} — data + all metadata. */
    FULL,
    /** fdatasync via {@code FileChannel.force(false)} — data + size, no timestamps. */
    DATA
}
