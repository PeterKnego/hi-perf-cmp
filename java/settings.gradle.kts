rootProject.name = "hi-perf-cmp-java"

include(
    "common",
    "network-rtt-tcp",
    "network-rtt-udp",
    "network-rtt-quic",
    "filesystem-write-fsync",
    "filesystem-write-fdatasync",
    "filesystem-write-prealloc",
    "filesystem-write-batch",
    "thread-handoff",
)
