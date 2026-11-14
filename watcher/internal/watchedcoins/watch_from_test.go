package watchedcoins_test

import (
	"context"
	"errors"
	"testing"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
	"fairchance/watcher/internal/watchedcoins"
)

// launched at block 20, head cursor 50 (the final head is block 49).
func launchedSet(src eventsource.Source, fake *telegram.Fake) *watchedcoins.Set {
	set := newSet(src, fake)
	set.Facts = func(context.Context, string) (eventsource.CoinFacts, error) {
		return eventsource.CoinFacts{Symbol: "MOTH", LaunchBlock: 20}, nil
	}
	return set
}

func TestWatchFromReadsTheCoinFromThatBlockOn(t *testing.T) {
	ctx := context.Background()
	fake := &telegram.Fake{}
	set := launchedSet(sourceByBlock{events: []eventsource.Event{capFilled("a.testnet", 19), capFilled("a.testnet", 25), capFilled("a.testnet", 45)}}, fake)

	if err := set.WatchFrom(ctx, "a.testnet", 25); err != nil {
		t.Fatal(err)
	}
	if err := set.Poll(ctx); err != nil {
		t.Fatal(err)
	}
	if len(fake.Sent) != 2 {
		t.Fatalf("sent %d alerts, want 2 (blocks 25 and 45): %+v", len(fake.Sent), fake.Sent)
	}
}

func TestWatchFromRefusesABlockBeforeLaunchOrAfterTheHead(t *testing.T) {
	ctx := context.Background()
	for _, tc := range []struct {
		block uint64
		want  error
	}{{19, watchedcoins.ErrBeforeLaunch}, {49, watchedcoins.ErrAfterHead}, {50, watchedcoins.ErrAfterHead}, {9000, watchedcoins.ErrAfterHead}} {
		set := launchedSet(sourceByBlock{}, &telegram.Fake{})
		err := set.WatchFrom(ctx, "a.testnet", tc.block)
		if !errors.Is(err, tc.want) {
			t.Errorf("block %d: err %v, want %v", tc.block, err, tc.want)
		}
		if len(set.Coins()) != 0 {
			t.Errorf("block %d: a refused block still started a watch", tc.block)
		}
	}
	for _, ok := range []uint64{20, 48} {
		if err := launchedSet(sourceByBlock{}, &telegram.Fake{}).WatchFrom(ctx, "a.testnet", ok); err != nil {
			t.Errorf("block %d: %v", ok, err)
		}
	}
}

func TestWatchFromMovesTheCursorOfACoinAlreadyWatched(t *testing.T) {
	ctx := context.Background()
	fake := &telegram.Fake{}
	set := launchedSet(sourceByBlock{events: []eventsource.Event{capFilled("a.testnet", 30)}}, fake)
	if err := set.Watch(ctx, "a.testnet"); err != nil { // cursor 50: block 30 is already past
		t.Fatal(err)
	}
	if err := set.WatchFrom(ctx, "a.testnet", 28); err != nil {
		t.Fatal(err)
	}
	_ = set.Poll(ctx)
	if len(fake.Sent) != 1 {
		t.Fatalf("sent %d, want 1", len(fake.Sent))
	}
}

// gatedSource holds a read open until release is closed, then reports a cursor far ahead.
type gatedSource struct {
	started chan struct{}
	release chan struct{}
	cursors []eventsource.Cursor
}

func (g *gatedSource) Events(_ context.Context, _ []string, since eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	g.cursors = append(g.cursors, since)
	if len(g.cursors) == 1 {
		close(g.started)
		<-g.release
	}
	return nil, 5000, nil
}

func TestAReadPassInFlightDoesNotOverwriteTheCursorOfAWatchFrom(t *testing.T) {
	ctx := context.Background()
	src := &gatedSource{started: make(chan struct{}), release: make(chan struct{})}
	set := launchedSet(src, &telegram.Fake{})
	if err := set.Watch(ctx, "a.testnet"); err != nil {
		t.Fatal(err)
	}
	done := make(chan error)
	go func() { done <- set.Poll(ctx) }()
	<-src.started
	if err := set.WatchFrom(ctx, "a.testnet", 28); err != nil {
		t.Fatal(err)
	}
	close(src.release)
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	if err := set.Poll(ctx); err != nil {
		t.Fatal(err)
	}
	if src.cursors[1] != 28 {
		t.Fatalf("the next read started at %d, want 28 from the WatchFrom", src.cursors[1])
	}
}
