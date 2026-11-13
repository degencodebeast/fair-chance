// Package watchedcoins holds the coins the bot watches, each with its own read cursor, and sends
// one alert per new event. It keeps a short history of what it sent, in memory only.
package watchedcoins

import (
	"context"
	"sort"
	"sync"
	"time"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

const historySize = 50

type Set struct {
	Source   eventsource.Source
	Head     func(ctx context.Context) (eventsource.Cursor, error)
	Facts    func(ctx context.Context, coin string) (eventsource.CoinFacts, error)
	Sender   telegram.Sender
	ChatID   int64
	Location *time.Location // for HH:MM times in alerts

	mu      sync.Mutex
	coins   map[string]*watched
	history []string
}

type watched struct {
	cursor eventsource.Cursor
	rules  alerts.CoinRules
}

// Watch starts alerts for a coin from the current head on. It fails when the coin's views cannot be read.
// Watching a coin twice keeps the first cursor.
func (s *Set) Watch(ctx context.Context, coin string) error {
	facts, err := s.Facts(ctx, coin)
	if err != nil {
		return err
	}
	head, err := s.Head(ctx)
	if err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.coins == nil {
		s.coins = map[string]*watched{}
	}
	if _, already := s.coins[coin]; !already {
		s.coins[coin] = &watched{cursor: head, rules: alerts.CoinRules{
			Creator: facts.Creator, LaunchBlock: facts.LaunchBlock, EarlyBlocks: facts.EarlyBlocks, Symbol: facts.Symbol,
		}}
	}
	return nil
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
		w.cursor = next
		s.mu.Unlock()
		if err != nil && firstErr == nil {
			firstErr = err
		}
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
