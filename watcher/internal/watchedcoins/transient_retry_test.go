package watchedcoins_test

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

// scriptedSource answers each call with the next scripted error (nil is a good read) and records the cursors it was given.
type scriptedSource struct {
	script  []error
	cursors []eventsource.Cursor
}

func (s *scriptedSource) Events(_ context.Context, _ []string, since eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	s.cursors = append(s.cursors, since)
	err := s.script[min(len(s.cursors)-1, len(s.script)-1)]
	if err != nil {
		return nil, since, err
	}
	return nil, since + 5, nil
}

func transient(status int) error {
	return fmt.Errorf("/v0/account: HTTP %d: %w", status, eventsource.ErrTransient)
}

func TestAShortFaultIsRetriedWithAGrowingPauseAndTheSameCursor(t *testing.T) {
	src := &scriptedSource{script: []error{transient(500)}}
	now := time.Date(2026, 11, 14, 12, 0, 0, 0, time.UTC)
	set := watching(t, src, func() time.Time { return now })

	pauses := []time.Duration{2, 4, 8, 16, 32, 60, 60}
	for i, pause := range pauses {
		pause *= time.Second
		calls := len(src.cursors)
		err := set.Poll(context.Background())
		if len(src.cursors) != calls+1 || err == nil {
			t.Fatalf("round %d: calls %d, err %v", i, len(src.cursors), err)
		}
		if !strings.Contains(err.Error(), "HTTP 500") || !strings.Contains(err.Error(), pause.String()) {
			t.Fatalf("round %d: error text %q must name the status and the wait %v", i, err, pause)
		}
		now = now.Add(pause - time.Second)
		_ = set.Poll(context.Background())
		if len(src.cursors) != calls+1 {
			t.Fatalf("round %d: read again before the pause %v ended", i, pause)
		}
		now = now.Add(time.Second)
	}
	for _, c := range src.cursors {
		if c != 50 {
			t.Fatalf("cursor %d, want 50 on every retry: %v", c, src.cursors)
		}
	}
	if set.Stopped() != nil {
		t.Fatal("a short fault must not stop the watcher")
	}
}

func TestEveryShortFaultKindIsRetriedAndAGoodReadResetsThePause(t *testing.T) {
	for _, status := range []int{500, 502, 503} {
		src := &scriptedSource{script: []error{transient(status), transient(status), nil, transient(status), nil}}
		now := time.Date(2026, 11, 14, 12, 0, 0, 0, time.UTC)
		set := watching(t, src, func() time.Time { return now })

		step := func(after time.Duration) {
			now = now.Add(after)
			_ = set.Poll(context.Background())
		}
		step(0)               // fault: pause 2 s
		step(2 * time.Second) // fault: pause 4 s
		step(4 * time.Second) // good read: pause resets, cursor moves on
		step(0)               // fault again: pause is 2 s, not 8 s
		before := len(src.cursors)
		step(time.Second)
		if len(src.cursors) != before {
			t.Fatalf("status %d: read during a 2 s pause", status)
		}
		step(time.Second)
		if len(src.cursors) != before+1 {
			t.Fatalf("status %d: the pause did not reset to 2 s after a good read", status)
		}
		if got := src.cursors[3]; got != 55 {
			t.Fatalf("status %d: cursor after a good read is %d, want 55", status, got)
		}
		if set.Stopped() != nil {
			t.Fatalf("status %d stopped the watcher", status)
		}
	}
}

func TestWrongSetupAnswersStillStopAllReads(t *testing.T) {
	for _, bad := range []error{eventsource.ErrForbidden, eventsource.ErrUnexpectedResponse, eventsource.ErrRPCForbidden, eventsource.ErrRPCWrongSetup} {
		src := &failingSource{err: fmt.Errorf("read: %w", bad)}
		set := watching(t, src, time.Now)
		for range 3 {
			_ = set.Poll(context.Background())
		}
		if src.calls != 1 || !errors.Is(set.Stopped(), bad) {
			t.Errorf("%v: %d calls, stopped %v", bad, src.calls, set.Stopped())
		}
	}
}

// A good read of coin A must not reset the pause that coin B's fault set.
func TestAGoodReadOfOneCoinDoesNotResetTheBackoffOfAFaultOnAnother(t *testing.T) {
	src := &perCoinSource{bad: "b.testnet"}
	now := time.Date(2026, 11, 14, 12, 0, 0, 0, time.UTC)
	set := newSet(src, &telegram.Fake{})
	set.Now = func() time.Time { return now }
	for _, c := range []string{"a.testnet", "b.testnet"} {
		if err := set.Watch(context.Background(), c); err != nil {
			t.Fatal(err)
		}
	}
	var waits []string
	for range 3 {
		err := set.Poll(context.Background())
		if err == nil {
			t.Fatal("want the fault of b")
		}
		waits = append(waits, err.Error())
		now = now.Add(time.Minute)
	}
	for i, want := range []string{"next read in 2s", "next read in 4s", "next read in 8s"} {
		if !strings.Contains(waits[i], want) {
			t.Fatalf("pass %d: %q, want %q (coin a read well and must not reset b's pause)", i, waits[i], want)
		}
	}
}

type perCoinSource struct{ bad string }

func (s *perCoinSource) Events(_ context.Context, coins []string, since eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	if coins[0] == s.bad {
		return nil, since, transient(503)
	}
	return nil, since, nil
}
