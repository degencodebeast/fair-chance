package watchedcoins_test

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
	"fairchance/watcher/internal/watchedcoins"
)

type sourceByBlock struct{ events []eventsource.Event }

func (s sourceByBlock) Events(_ context.Context, coins []string, since eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	var out []eventsource.Event
	for _, ev := range s.events {
		if eventsource.Cursor(ev.Block) >= since && ev.Coin == coins[0] {
			out = append(out, ev)
		}
	}
	return out, 100, nil
}

func capFilled(coin string, block uint64) eventsource.Event {
	data, _ := json.Marshal(map[string]string{"cap_used": "2130000000000000000000000", "window_end_ns": "1794582120000000000"})
	return eventsource.Event{Standard: "fair_chance", Name: "fc_cap_filled", Coin: coin, Block: block, Data: data}
}

func newSet(src eventsource.Source, fake *telegram.Fake) *watchedcoins.Set {
	return &watchedcoins.Set{
		Source: src,
		Head:   func(context.Context) (eventsource.Cursor, error) { return 50, nil },
		Facts: func(context.Context, string) (eventsource.CoinFacts, error) {
			return eventsource.CoinFacts{Symbol: "MOTH"}, nil
		},
		Sender:   fake,
		ChatID:   7,
		Location: time.UTC,
	}
}

func TestAWatchedCoinSendsAlertsForNewEventsOnlyAndNeverTwice(t *testing.T) {
	ctx := context.Background()
	fake := &telegram.Fake{}
	set := newSet(sourceByBlock{events: []eventsource.Event{capFilled("a.testnet", 40), capFilled("a.testnet", 55), capFilled("b.testnet", 56)}}, fake)

	if err := set.Watch(ctx, "a.testnet"); err != nil {
		t.Fatal(err)
	}
	for range 2 {
		if err := set.Poll(ctx); err != nil {
			t.Fatal(err)
		}
	}

	want := "Pool cap full for this hour: 2.13 NEAR out. Limited sells resume at 15:02."
	if len(fake.Sent) != 1 || fake.Sent[0].Text != want || fake.Sent[0].ChatID != 7 {
		t.Fatalf("sent %+v", fake.Sent)
	}
	if recent := set.Recent(); len(recent) != 1 || recent[0] != want {
		t.Fatalf("recent %v", recent)
	}
}

func TestACoinThatCannotBeReadIsNotWatched(t *testing.T) {
	fake := &telegram.Fake{}
	set := newSet(sourceByBlock{}, fake)
	set.Facts = func(context.Context, string) (eventsource.CoinFacts, error) {
		return eventsource.CoinFacts{}, errors.New("no such account")
	}
	if err := set.Watch(context.Background(), "nothing.testnet"); err == nil {
		t.Fatal("want an error")
	}
	if got := set.Coins(); len(got) != 0 {
		t.Fatalf("watching %v", got)
	}
}

func TestTheAlertKeepsTheCoinsSymbolFromItsFacts(t *testing.T) {
	ctx := context.Background()
	fake := &telegram.Fake{}
	data, _ := json.Marshal(map[string]string{"account": "bigfish.testnet", "class": "large", "tokens_asked": "30000000000000000000000000", "tokens_sold": "100000000000000000000000", "fill": "partial", "limit_reason": "cap_full", "allowance_left": "0", "window_end_ns": "1794582120000000000"})
	src := sourceByBlock{events: []eventsource.Event{{Standard: "fair_chance", Name: "fc_sell", Coin: "a.testnet", Block: 60, Data: data}}}
	set := newSet(src, fake)
	_ = set.Watch(ctx, "a.testnet")
	_ = set.Poll(ctx)
	if len(fake.Sent) != 1 || !strings.Contains(fake.Sent[0].Text, "30.0M MOTH") {
		t.Fatalf("sent %+v", fake.Sent)
	}
}
