package main

import "sync/atomic"

// spsc is a bounded single-producer single-consumer ring of uint64 tokens with
// busy-wait (no parking). head/tail are monotonic; head doubles as the consumed
// count. Atomic Load/Store establish the happens-before for the plain buf slots.
type spsc struct {
	buf  []uint64
	cap  uint64
	head atomic.Uint64 // total popped (consumer)
	tail atomic.Uint64 // total pushed (producer)
}

func newSPSC(capacity int) *spsc {
	return &spsc{buf: make([]uint64, capacity), cap: uint64(capacity)}
}

func (s *spsc) push(v uint64) {
	tail := s.tail.Load()
	for tail-s.head.Load() == s.cap {
	}
	s.buf[tail%s.cap] = v
	s.tail.Store(tail + 1)
}

func (s *spsc) pop() uint64 {
	head := s.head.Load()
	for head == s.tail.Load() {
	}
	v := s.buf[head%s.cap]
	s.head.Store(head + 1)
	return v
}

func (s *spsc) consumed() uint64 {
	return s.head.Load()
}
