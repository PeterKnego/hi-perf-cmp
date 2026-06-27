package main

import "sync/atomic"

// cacheLine is the assumed CPU cache-line size; index fields are padded to this
// so the producer-written and consumer-written lines never ping-pong (false
// sharing). atomic.Uint64 is 8 bytes and each owning field is 8 bytes, so 64-16
// bytes of filler fill out the rest of the line.
const cacheLine = 64

// spsc is a bounded single-producer single-consumer ring of uint64 tokens with
// busy-wait (no parking). head/tail are monotonic; head doubles as the consumed
// count. Atomic Load/Store establish the happens-before for the plain buf slots.
//
// Two LMAX-style optimizations:
//  1. False-sharing elimination — the producer-owned line (tail, cachedHead) and
//     the consumer-owned line (head, cachedTail) are each padded to a full cache
//     line and separated by another, so push and pop touch different lines.
//  2. Cached opposite index — push keeps a producer-local snapshot of head
//     (cachedHead) and only re-Loads the real head when the ring *appears* full;
//     pop keeps a consumer-local snapshot of tail (cachedTail) and only re-Loads
//     the real tail when it *appears* empty. The snapshot is always <= the real
//     value, so "appears full/empty by cache" is conservative and safe.
//
// cachedHead is single-owner (producer only); cachedTail is single-owner
// (consumer only). Neither is read+written by both goroutines.
type spsc struct {
	buf []uint64
	cap uint64
	_   [cacheLine - 16]byte // isolate the read-only header from the producer line

	// Producer-owned cache line.
	tail       atomic.Uint64 // total pushed (producer)
	cachedHead uint64        // producer-local snapshot of head (<= real head)
	_          [cacheLine - 16]byte

	// Consumer-owned cache line.
	head       atomic.Uint64 // total popped (consumer); also the consumed count
	cachedTail uint64        // consumer-local snapshot of tail (<= real tail)
	_          [cacheLine - 16]byte
}

func newSPSC(capacity int) *spsc {
	return &spsc{buf: make([]uint64, capacity), cap: uint64(capacity)}
}

func (s *spsc) push(v uint64) {
	tail := s.tail.Load()
	// Full by the cached head? Re-Load the real head until space appears.
	// cachedHead <= real head, so tail-cachedHead < cap guarantees not full.
	if tail-s.cachedHead == s.cap {
		for {
			s.cachedHead = s.head.Load()
			if tail-s.cachedHead < s.cap {
				break
			}
		}
	}
	s.buf[tail%s.cap] = v
	s.tail.Store(tail + 1) // publishes the slot write above
}

func (s *spsc) pop() uint64 {
	head := s.head.Load()
	// Empty by the cached tail? Re-Load the real tail until an item appears.
	// cachedTail <= real tail, so head != cachedTail guarantees an item exists.
	if head == s.cachedTail {
		for {
			s.cachedTail = s.tail.Load() // observes the producer's slot write
			if head != s.cachedTail {
				break
			}
		}
	}
	v := s.buf[head%s.cap]
	s.head.Store(head + 1)
	return v
}

func (s *spsc) consumed() uint64 {
	return s.head.Load()
}
