package timing_test

import (
	"math"
	"testing"

	"fairchance/watcher/internal/timing"
)

func near(a, b float64) bool { return math.Abs(a-b) < 1e-9 }

func TestTheLatencyIsTheFirstReadTimeMinusTheReceiptBlockTime(t *testing.T) {
	s := timing.Seen{ReceiptBlockNs: 1_000_000_000_000, SeenAtNs: 1_002_500_000_000}
	if got := s.LatencySeconds(); !near(got, 2.5) {
		t.Fatalf("latency %v, want 2.5", got)
	}
}

func TestAReadBeforeTheBlockTimeIsANegativeLatencyNotAWrapAround(t *testing.T) {
	// The local clock can run behind the chain; the report shows that as it is.
	s := timing.Seen{ReceiptBlockNs: 2_000_000_000, SeenAtNs: 1_500_000_000}
	if got := s.LatencySeconds(); !near(got, -0.5) {
		t.Fatalf("latency %v, want -0.5", got)
	}
}

func TestSecondsPerBlockIsTheTimeBetweenTwoBlocksOverTheirHeightGap(t *testing.T) {
	got, err := timing.SecondsPerBlock(100, 10_000_000_000, 120, 22_000_000_000)
	if err != nil || !near(got, 0.6) {
		t.Fatalf("got %v %v, want 0.6", got, err)
	}
	if _, err := timing.SecondsPerBlock(120, 1, 120, 2); err == nil {
		t.Fatal("no height gap was accepted")
	}
	if _, err := timing.SecondsPerBlock(120, 5, 100, 9); err == nil {
		t.Fatal("a later block with a lower height was accepted")
	}
}

func TestTheSummaryGivesCountMinMedianAndMax(t *testing.T) {
	var seen []timing.Seen
	for _, ms := range []uint64{3000, 1000, 2000, 5000} {
		seen = append(seen, timing.Seen{ReceiptBlockNs: 0, SeenAtNs: ms * 1_000_000})
	}
	got := timing.Summarize(seen)
	if got.Count != 4 || !near(got.MinSeconds, 1) || !near(got.MedianSeconds, 2.5) || !near(got.MaxSeconds, 5) {
		t.Fatalf("summary %+v", got)
	}
	if empty := timing.Summarize(nil); empty.Count != 0 {
		t.Fatalf("empty summary %+v", empty)
	}
}

func TestTheBackOffDoublesUpToTheLimitAndTakesALongerRetryAfter(t *testing.T) {
	b := timing.BackOff{Start: 1, Max: 8}
	var got []int64
	for range 5 {
		got = append(got, int64(b.Next(0)))
	}
	want := []int64{1, 2, 4, 8, 8}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("waits %v, want %v", got, want)
		}
	}
	if w := b.Next(20); w != 20 {
		t.Fatalf("Retry-After 20 gave %d", w)
	}
	b.Reset()
	if w := b.Next(0); w != 1 {
		t.Fatalf("after reset %d", w)
	}
}

func TestTheLimitedBackOffGivesAtMostTwoWaitsWithinTheTotal(t *testing.T) {
	b := timing.LimitedBackOff{BackOff: timing.BackOff{Start: 2, Max: 60}, MaxWaits: 2, Total: 900}
	if w, ok := b.Next(0); !ok || w != 2 {
		t.Fatalf("first %d %v", w, ok)
	}
	if w, ok := b.Next(561); !ok || w != 561 {
		t.Fatalf("second %d %v", w, ok)
	}
	if _, ok := b.Next(0); ok {
		t.Fatal("a third wait")
	}
	long := timing.LimitedBackOff{BackOff: timing.BackOff{Start: 2, Max: 60}, MaxWaits: 2, Total: 900}
	if _, ok := long.Next(901); ok {
		t.Fatal("a wait past the total")
	}
}
