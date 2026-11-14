// Package watchedcoins holds the coins the bot watches, each with its own read cursor, and sends
// one alert per new event. It keeps a short history of what it sent, in memory only.
package watchedcoins

import (
	"context"
	"errors"
	"fmt"
	"sort"
	"sync"
	"time"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

const (
	historySize = 50
	// A 429 or a short fault (HTTP 5xx, timeout, transport error) pauses reads for 2 s, then doubles each time up to 60 s.
	firstBackoff = 2 * time.Second
	maxBackoff   = 60 * time.Second
)

type Set struct {
	Source   eventsource.Source
	Head     func(ctx context.Context) (eventsource.Cursor, error)
	Facts    func(ctx context.Context, coin string) (eventsource.CoinFacts, error)
	Sender   telegram.Sender
	ChatID   int64
	Location *time.Location // for HH:MM times in alerts
	Now      func() time.Time

	mu         sync.Mutex
	stopped    error         // set by a 403 or an unknown answer: no read after it
	pausedTill time.Time     // set by a 429 or a short fault
	backoff    time.Duration // the last step after a 429 or short fault; 0 after a good read
	coins      map[string]*watched
	history    []string
}

type watched struct {
	cursor eventsource.Cursor
	rules  alerts.CoinRules
}

var (
	// ErrBeforeLaunch is a start block below the coin's launch block.
	ErrBeforeLaunch = errors.New("the start block is before the launch block of the coin")
	// ErrAfterHead is a start block after the final head.
	ErrAfterHead = errors.New("the start block is after the final head")
)

// Watch starts alerts for a coin from the current head on. It fails when the coin's views cannot be read.
// Watching a coin twice keeps the first cursor.
func (s *Set) Watch(ctx context.Context, coin string) error {
	return s.start(ctx, coin, nil)
}

// WatchFrom starts alerts for a coin from a block on, to read past events. The block must be the
// coin's launch block or later, and before the final head (ErrBeforeLaunch, ErrAfterHead; /watch alone starts from now). When
// the coin is already watched, its cursor moves to the block.
func (s *Set) WatchFrom(ctx context.Context, coin string, block uint64) error {
	return s.start(ctx, coin, &block)
}

func (s *Set) start(ctx context.Context, coin string, from *uint64) error {
	facts, err := s.Facts(ctx, coin)
	if err != nil {
		return err
	}
	head, err := s.Head(ctx) // the block after the final head
	if err != nil {
		return err
	}
	cursor := head
	if from != nil {
		if *from < facts.LaunchBlock {
			return fmt.Errorf("%w: block %d, launch block %d", ErrBeforeLaunch, *from, facts.LaunchBlock)
		}
		if eventsource.Cursor(*from)+1 >= head { // head is the block after the final head; a watch from now on is /watch alone
			return fmt.Errorf("%w: block %d, final head %d", ErrAfterHead, *from, head-1)
		}
		cursor = eventsource.Cursor(*from)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.coins == nil {
		s.coins = map[string]*watched{}
	}
	if w, already := s.coins[coin]; !already {
		s.coins[coin] = &watched{cursor: cursor, rules: alerts.CoinRules{
			Creator: facts.Creator, LaunchBlock: facts.LaunchBlock, EarlyBlocks: facts.EarlyBlocks, Symbol: facts.Symbol,
		}}
	} else if from != nil {
		w.cursor = cursor
	}
	return nil
}

// Stopped is nil until a read stops for good.
func (s *Set) Stopped() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.stopped
}

// Coins lists the watched coin accounts.
func (s *Set) Coins() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make([]string, 0, len(s.coins))
	for c := range s.coins {
		out = append(out, c)
	}
	sort.Strings(out)
	return out
}

// Recent gives the last alerts sent, oldest first.
func (s *Set) Recent() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]string(nil), s.history...)
}

// Poll reads new events of every watched coin once and sends their alerts.
func (s *Set) Poll(ctx context.Context) error {
	if err := s.Stopped(); err != nil {
		return err
	}
	if s.paused() {
		return nil
	}
	var firstErr error
	for _, coin := range s.Coins() {
		s.mu.Lock()
		w := s.coins[coin]
		cursor := w.cursor
		s.mu.Unlock()

		writer := alerts.Writer{
			Location: s.Location,
			Rules:    func(string) (alerts.CoinRules, bool) { return w.rules, true },
		}
		sender := &recordingSender{inner: s.Sender, set: s}
		_, next, err := alerts.SendNew(ctx, s.Source, []string{coin}, cursor, writer, sender, s.ChatID)

		s.mu.Lock()
		if w.cursor == cursor { // a WatchFrom during the read moved the cursor: keep its choice
			w.cursor = next
		}
		s.mu.Unlock()
		if err = s.noteFailure(err); s.Stopped() != nil {
			return err
		}
		if s.paused() {
			return err // no read of the other coins during the wait
		}
		if err != nil && firstErr == nil {
			firstErr = err
		}
	}
	if firstErr == nil {
		s.noteGoodRead() // a full pass with no fault ends the run of faults
	}
	return firstErr
}

type recordingSender struct {
	inner telegram.Sender
	set   *Set
}

func (r *recordingSender) Send(ctx context.Context, chatID int64, text string) error {
	if err := r.inner.Send(ctx, chatID, text); err != nil {
		return err
	}
	r.set.mu.Lock()
	defer r.set.mu.Unlock()
	r.set.history = append(r.set.history, text)
	if len(r.set.history) > historySize {
		r.set.history = r.set.history[len(r.set.history)-historySize:]
	}
	return nil
}

func (s *Set) now() time.Time {
	if s.Now != nil {
		return s.Now()
	}
	return time.Now()
}

func (s *Set) paused() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.now().Before(s.pausedTill)
}

// noteGoodRead ends a run of short faults: the next fault pauses for 2 s again.
func (s *Set) noteGoodRead() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.backoff = 0
}

// pause grows the shared wait (2 s doubling to 60 s) and pauses reads for it, or for retryAfter when that is
// longer. A 429 and a short fault share the one counter: they come from the same provider and its rate budget.
// The caller holds s.mu.
func (s *Set) pause(err error, retryAfter time.Duration) error {
	s.backoff = min(max(2*s.backoff, firstBackoff), maxBackoff)
	wait := max(s.backoff, retryAfter)
	s.pausedTill = s.now().Add(wait)
	return fmt.Errorf("%w; next read in %s", err, wait)
}

// noteFailure applies the source rules to a read error and returns the error to report.
// A 403, a wrong-setup answer or a bad URL stops all reads for good, with no retry.
// A 429 or a short fault (5xx, timeout, transport error) pauses reads with a growing wait, 2 s doubling to
// 60 s, or the Retry-After wait of a 429 when that is longer. The same cursor reads again after it.
// The returned error names the wait.
func (s *Set) noteFailure(err error) error {
	if err == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case errors.Is(err, eventsource.ErrForbidden), errors.Is(err, eventsource.ErrUnexpectedResponse),
		errors.Is(err, eventsource.ErrRPCForbidden), errors.Is(err, eventsource.ErrRPCWrongSetup):
		s.stopped = err
	case errors.Is(err, eventsource.ErrTransient):
		return s.pause(err, 0)
	case errors.Is(err, eventsource.ErrRateLimited):
		var limited *eventsource.RateLimitedError
		var retryAfter time.Duration
		if errors.As(err, &limited) {
			retryAfter = limited.RetryAfter
		}
		return s.pause(err, retryAfter)
	}
	return err
}
