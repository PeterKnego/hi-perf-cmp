package serjournal

import "testing"

// Golden values generated from the Rust serialization-common implementation
// (checksum_record(build_record(index, entries, textLen))) on 2026-07-16.
// To regenerate: build a scratch crate depending on rust/serialization/common
// and print the checksums for the tuples below (see the implementation plan).
var golden = []struct {
	index             uint64
	entries, textLen  int
	want              uint64
}{
	{0, 4, 78, 0x86d721cbffdefc06},
	{1, 4, 78, 0xddb1bfa73e9819cb},
	{42, 4, 78, 0x495a0d763cc820ca},
	{99999, 4, 78, 0x552b92436dae830e},
	{7, 2, 8, 0x9b525460dd070517},
}

func TestGoldenChecksumsMatchRust(t *testing.T) {
	for _, g := range golden {
		r := BuildRecord(g.index, g.entries, g.textLen)
		if got := ChecksumRecord(&r); got != g.want {
			t.Errorf("(%d,%d,%d): got %#016x, want %#016x",
				g.index, g.entries, g.textLen, got, g.want)
		}
	}
}

func TestBuildRecordIsDeterministic(t *testing.T) {
	a := BuildRecord(42, 4, 78)
	b := BuildRecord(42, 4, 78)
	if ChecksumRecord(&a) != ChecksumRecord(&b) {
		t.Fatal("same index produced different records")
	}
	if len(a.Entries) != 4 || len(a.Entries[0].CmdText) != 78 {
		t.Fatalf("unexpected shape: %d entries, %d command bytes",
			len(a.Entries), len(a.Entries[0].CmdText))
	}
}

func TestBuildRecordVariesByIndex(t *testing.T) {
	a := BuildRecord(1, 4, 78)
	b := BuildRecord(2, 4, 78)
	if ChecksumRecord(&a) == ChecksumRecord(&b) {
		t.Fatal("different indices produced identical records")
	}
}
