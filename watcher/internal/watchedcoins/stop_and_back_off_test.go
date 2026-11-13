package watchedcoins_test

import (
	"context"
	"fmt"
	"testing"
	"time"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
	"fairchance/watcher/internal/watchedcoins"
)

// failingSource counts its calls and always returns err.
type failingSource struct {
	err   error
	calls int
}

func (s *failingSource) Events(context.Context, []string, eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	s.calls++
	return nil, 0, s.err
}

func watching(t *testing.T, src eventsource.Source, now func() time.Time) *watchedcoins.Set {
	t.Helper()
	set := newSet(src, &telegram.Fake{})
	set.Now = now
	if err := set.Watch(context.Background(), "a.testnet"); err != nil {
		t.Fatal(err)
	}
	return set
}

func TestAForbiddenAnswerStopsAllReadsAtOnce(t *testing.T) {
	for _, bad := range []error{eventsource.ErrForbidden, eventsource.ErrUnexpectedResponse} {
		src := &failingSource{err: fmt.Errorf("read: %w", bad)}
		set := watching(t, src, time.Now)

		_ = set.Poll(context.Background())
		for range 3 {
			_ = set.Poll(context.Background())
		}
		if src.calls != 1 {
			t.Errorf("%v: %d source calls, want 1", bad, src.calls)
		}
		if set.Stopped() == nil {
			t.Errorf("%v: Stopped() is nil", bad)
		}
	}
}

func TestARateLimitPausesReadsForAtLeastRetryAfter(t *testing.T) {
	src := &failingSource{err: &eventsource.RateLimitedError{RetryAfter: 90 * time.Second}}
	now := time.Date(2026, 11, 13, 12, 0, 0, 0, time.UTC)
	set := watching(t, src, func() time.Time { return now })

	_ = set.Poll(context.Background())
	now = now.Add(89 * time.Second)
	_ = set.Poll(context.Background())
	if src.calls != 1 {
		t.Fatalf("%d calls after 89 s, want 1", src.calls)
	}
	now = now.Add(2 * time.Second)
	_ = set.Poll(context.Background())
	if src.calls != 2 {
		t.Fatalf("%d calls after 91 s, want 2", src.calls)
	}
	if set.Stopped() != nil {
		t.Fatal("a rate limit must not stop the watcher")
	}
}
