package smrcoll

// SmrSeed is the fixed workload seed (plan Appendix A.2), identical across langs.
const SmrSeed uint64 = 0x123456789ABCDEF0

// SplitMix is a splitmix64 generator.
type SplitMix struct{ state uint64 }

func NewSplitMix(seed uint64) *SplitMix { return &SplitMix{state: seed} }

func (s *SplitMix) Next() uint64 {
	s.state += 0x9E3779B97F4A7C15
	z := s.state
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// Insert / Update workload draws (plan Appendix A.3).
type InsertOp struct {
	OrderID, Price, Qty int64
	Side                uint8
}
type UpdateOp struct {
	OrderID, FillQty int64
}

func NextInsert(rng *SplitMix, i int, nLevels uint32, tick, priceMin int64) InsertOp {
	r1 := rng.Next()
	r2 := rng.Next()
	t := int64(r1 % uint64(nLevels))
	side := uint8((r1 >> 32) & 1)
	return InsertOp{
		OrderID: int64(i) + 1,
		Price:   priceMin + t*tick,
		Qty:     1 + int64(r2%1000),
		Side:    side,
	}
}

func NextUpdate(rng *SplitMix, n int) UpdateOp {
	u := rng.Next()
	return UpdateOp{
		OrderID: int64(u%uint64(n)) + 1,
		FillQty: 1 + int64((u>>32)%100),
	}
}
