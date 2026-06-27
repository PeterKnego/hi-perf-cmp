// thread-handoff-condvar (Go): sync.Cond rendezvous. Isolates park/unpark +
// signal cost. Emits three handoff_rtt_* lines.
package main

import (
	"sync"

	"github.com/peterknego/hi-perf-cmp/go/internal/bench"
)

const experiment = "condvar"

// mailbox is a one-slot rendezvous carrying a single token.
type mailbox struct {
	mu   sync.Mutex
	cond *sync.Cond
	val  uint64
	full bool
}

func newMailbox() *mailbox {
	m := &mailbox{}
	m.cond = sync.NewCond(&m.mu)
	return m
}

func (m *mailbox) send(v uint64) {
	m.mu.Lock()
	m.val = v
	m.full = true
	m.mu.Unlock()
	m.cond.Signal()
}

func (m *mailbox) recv() uint64 {
	m.mu.Lock()
	for !m.full {
		m.cond.Wait()
	}
	v := m.val
	m.full = false
	m.mu.Unlock()
	return v
}

func main() {
	cfg, err := bench.LoadHandoffConfig()
	if err != nil {
		bench.Fatalf("thread-handoff-"+experiment, "%v", err)
	}
	total := cfg.Warmup + cfg.Iterations

	req, resp := newMailbox(), newMailbox()

	done := make(chan struct{})
	go func() {
		for i := 0; i < total; i++ {
			resp.send(req.recv())
		}
		close(done)
	}()

	samples := bench.MeasureHandoff(cfg, func() {
		req.send(1)
		_ = resp.recv()
	})

	<-done
	bench.EmitHandoff(experiment, samples)
}
