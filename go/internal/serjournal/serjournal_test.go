package serjournal

import "testing"

// Golden values generated from the Rust serialization-common implementation
// (checksum_record(build_record(index, entries, cmdBytes))) on 2026-07-16.
// To regenerate: build a scratch crate depending on rust/serialization/common
// and print the checksums for the tuples below (see the implementation plan).
var golden = []struct {
	index             uint64
	entries, cmdBytes int
	want              uint64
}{
	{0, 4, 78, 0x7b8ca2b4f6f556d9},
	{1, 4, 78, 0x2ecb381439a319d6},
	{42, 4, 78, 0xe0e5b9514969d90d},
	{99999, 4, 78, 0xd19fa98130a517fe},
	{7, 2, 8, 0x6d62ff2cced105df},
}

func TestGoldenChecksumsMatchRust(t *testing.T) {
	for _, g := range golden {
		r := BuildRecord(g.index, g.entries, g.cmdBytes)
		if got := ChecksumRecord(&r); got != g.want {
			t.Errorf("(%d,%d,%d): got %#016x, want %#016x",
				g.index, g.entries, g.cmdBytes, got, g.want)
		}
	}
}

func TestBuildRecordIsDeterministic(t *testing.T) {
	a := BuildRecord(42, 4, 78)
	b := BuildRecord(42, 4, 78)
	if ChecksumRecord(&a) != ChecksumRecord(&b) {
		t.Fatal("same index produced different records")
	}
	if len(a.Entries) != 4 || len(a.Entries[0].Command) != 78 {
		t.Fatalf("unexpected shape: %d entries, %d command bytes",
			len(a.Entries), len(a.Entries[0].Command))
	}
}

func TestBuildRecordVariesByIndex(t *testing.T) {
	a := BuildRecord(1, 4, 78)
	b := BuildRecord(2, 4, 78)
	if ChecksumRecord(&a) == ChecksumRecord(&b) {
		t.Fatal("different indices produced identical records")
	}
}
