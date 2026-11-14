package watchedcoins_test

import (
	"context"
	"fmt"
	"strings"
	"testing"
	"time"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

func limited(after time.Duration) error {
	return fmt.Errorf("/v0/account: HTTP 429: %w", &eventsource.RateLimitedError{RetryAfter: after})
}

// A 429 with no Retry-After waits 2, 4, 8, 16, 32 s, then 60 s at most. The cursor stays the same.
func TestA429WaitGrowsFrom2To60SecondsWithTheSameCursor(t *testing.T) {
	src := &scriptedSource{script: []error{limited(0)}}
	now := time.Date(2026, 11, 14, 12, 0, 0, 0, time.UTC)
	set := watching(t, src, func() time.Time { return now })
	for i, pause := range []time.Duration{2, 4, 8, 16, 32, 60, 60} {
		pause *= time.Second
		calls := len(src.cursors)
		err := set.Poll(context.Background())
		if len(src.cursors) != calls+1 || err == nil || !strings.Contains(err.Error(), "next read in "+pause.String()) {
			t.Fatalf("round %d: calls %d, err %v, want a wait of %v in the text", i, len(src.cursors), err, pause)
		}
		now = now.Add(pause - time.Second)
		_ = set.Poll(context.Background())
		if len(src.cursors) != calls+1 {
			t.Fatalf("round %d: read before the wait %v ended", i, pause)
		}
		now = now.Add(time.Second)
	}
	for _, c := range src.cursors {
		if c != 50 {
			t.Fatalf("cursor %d, want 50: %v", c, src.cursors)
		}
	}
	if set.Stopped() != nil {
		t.Fatal("a 429 must not stop reads")
	}
}

// A longer Retry-After wins. A shorter one gives way to the step.
func TestALongerRetryAfterWinsOverTheStep(t *testing.T) {
	src := &scriptedSource{script: []error{limited(90 * time.Second), limited(time.Second), nil}}
	now := time.Date(2026, 11, 14, 12, 0, 0, 0, time.UTC)
	set := watching(t, src, func() time.Time { return now })

	err := set.Poll(context.Background())
	if err == nil || !strings.Contains(err.Error(), "next read in 1m30s") {
		t.Fatalf("first wait: %v", err)
	}
	now = now.Add(89 * time.Second)
	_ = set.Poll(context.Background())
	if len(src.cursors) != 1 {
		t.Fatal("read before Retry-After ended")
	}
	now = now.Add(time.Second)
	err = set.Poll(context.Background()) // second 429 with 1 s: the step is 4 s
	if err == nil || !strings.Contains(err.Error(), "next read in 4s") {
		t.Fatalf("second wait: %v", err)
	}
}

// 429 and 5xx share one counter: one provider, one rate budget.
func TestA429And5xxShareOneCounterAndAGoodPassResetsIt(t *testing.T) {
	src := &scriptedSource{script: []error{limited(0), transient(503), nil, limited(0)}}
	now := time.Date(2026, 11, 14, 12, 0, 0, 0, time.UTC)
	set := watching(t, src, func() time.Time { return now })
	want := []string{"next read in 2s", "next read in 4s", "", "next read in 2s"}
	waits := []time.Duration{2, 4, 0, 2}
	for i := range want {
		err := set.Poll(context.Background())
		if want[i] == "" {
			if err != nil {
				t.Fatalf("pass %d: %v", i, err)
			}
		} else if err == nil || !strings.Contains(err.Error(), want[i]) {
			t.Fatalf("pass %d: %v, want %q", i, err, want[i])
		}
		now = now.Add(waits[i] * time.Second)
	}
}

// An event that a 429 pause delays is sent once after the pause.
func TestNoEventIsLostAcrossA429Pause(t *testing.T) {
	src := &rateLimitedOnce{ev: capFilled("a.testnet", 55)}
	now := time.Date(2026, 11, 14, 12, 0, 0, 0, time.UTC)
	fake := &telegram.Fake{}
	set := newSet(src, fake)
	set.Now = func() time.Time { return now }
	if err := set.Watch(context.Background(), "a.testnet"); err != nil {
		t.Fatal(err)
	}
	_ = set.Poll(context.Background())
	if len(fake.Sent) != 0 {
		t.Fatal("sent during a 429")
	}
	now = now.Add(2 * time.Second)
	if err := set.Poll(context.Background()); err != nil {
		t.Fatal(err)
	}
	now = now.Add(time.Minute)
	_ = set.Poll(context.Background())
	if len(fake.Sent) != 1 {
		t.Fatalf("%d alerts, want 1", len(fake.Sent))
	}
}

type rateLimitedOnce struct {
	ev    eventsource.Event
	calls int
}

func (s *rateLimitedOnce) Events(_ context.Context, _ []string, since eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	s.calls++
	if s.calls == 1 {
		return nil, since, limited(0)
	}
	if since > eventsource.Cursor(s.ev.Block) {
		return nil, since, nil
	}
	return []eventsource.Event{s.ev}, eventsource.Cursor(s.ev.Block) + 1, nil
}
