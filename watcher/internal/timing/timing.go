// Package timing measures how long FastNEAR takes to show an event: from the time of the block that
// holds the event's receipt to the first FastNEAR read that returns it.
package timing

import (
	"errors"
	"slices"
	"time"
)

// Seen is one event: its receipt block and the local time of the first read that showed it.
type Seen struct {
	Event          string `json:"event"`
	ReceiptID      string `json:"receipt_id"`
	TxHash         string `json:"tx_hash"`
	ReceiptBlock   uint64 `json:"receipt_block"`
	ReceiptBlockNs uint64 `json:"receipt_block_ns"`
	SeenAtNs       uint64 `json:"seen_at_ns"`
}

// LatencySeconds is the first-read time minus the receipt block time. It is negative when the local
// clock runs behind the chain; the report keeps that as it is.
func (s Seen) LatencySeconds() float64 {
	return float64(int64(s.SeenAtNs)-int64(s.ReceiptBlockNs)) / 1e9
}

// SecondsPerBlock is the time between two blocks over their height gap.
func SecondsPerBlock(fromHeight, fromNs, toHeight, toNs uint64) (float64, error) {
	if toHeight <= fromHeight || toNs <= fromNs {
		return 0, errors.New("the second block must be later and higher than the first")
	}
	return float64(toNs-fromNs) / 1e9 / float64(toHeight-fromHeight), nil
}

// Summary of the latencies, in seconds.
type Summary struct {
	Count         int     `json:"count"`
	MinSeconds    float64 `json:"min_s"`
	MedianSeconds float64 `json:"median_s"`
	MaxSeconds    float64 `json:"max_s"`
}

func Summarize(seen []Seen) Summary {
	if len(seen) == 0 {
		return Summary{}
	}
	l := make([]float64, len(seen))
	for i, s := range seen {
		l[i] = s.LatencySeconds()
	}
	slices.Sort(l)
	mid := len(l) / 2
	median := l[mid]
	if len(l)%2 == 0 {
		median = (l[mid-1] + l[mid]) / 2
	}
	return Summary{Count: len(l), MinSeconds: l[0], MedianSeconds: median, MaxSeconds: l[len(l)-1]}
}

// BackOff gives the wait after each 429: Start, then double, up to Max. A longer Retry-After wins.
type BackOff struct {
	Start, Max time.Duration
	next       time.Duration
}

func (b *BackOff) Next(retryAfter time.Duration) time.Duration {
	if b.next == 0 {
		b.next = b.Start
	}
	wait := b.next
	b.next = min(b.next*2, b.Max)
	return max(wait, retryAfter)
}

func (b *BackOff) Reset() { b.next = 0 }

// LimitedBackOff is the builder's 429 rule: at most MaxWaits waits, and at most Total waited in
// all. Next gives false when the next wait would break either limit; the caller then stops and
// reports. The counts never reset, so a 200 between two 429s does not buy a third wait.
type LimitedBackOff struct {
	BackOff  BackOff
	MaxWaits int
	Total    time.Duration
	waits    int
	waited   time.Duration
}

func (b *LimitedBackOff) Next(retryAfter time.Duration) (time.Duration, bool) {
	if b.waits >= b.MaxWaits {
		return 0, false
	}
	wait := b.BackOff.Next(retryAfter)
	if b.waited+wait > b.Total {
		return 0, false
	}
	b.waits++
	b.waited += wait
	return wait, true
}

// Reset starts the doubling again after a good answer; the limits keep their counts.
func (b *LimitedBackOff) Reset() { b.BackOff.Reset() }
