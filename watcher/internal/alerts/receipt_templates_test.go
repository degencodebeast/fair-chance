package alerts_test

import (
	"encoding/json"
	"strings"
	"testing"
	"time"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
)

// Event JSON in this file is hand-written from the field names in docs/INTERFACE.md.
// Token amounts are base units (18 decimals); NEAR amounts are yoctoNEAR (24 decimals).
// 15:02 UTC on 2026-11-13 is 1794582120000000000 ns.
const (
	at1502 = "1794582120000000000"
	coinID = "moth.factory.testnet"
)

func testWriter() alerts.Writer {
	return alerts.Writer{
		Location: time.UTC,
		Rules: func(coin string) (alerts.CoinRules, bool) {
			if coin != coinID {
				return alerts.CoinRules{}, false
			}
			return alerts.CoinRules{Creator: "wick.lamp.testnet", LaunchBlock: 100, EarlyBlocks: 17, Symbol: "MOTH"}, true
		},
	}
}

func event(name string, block uint64, fields map[string]any) eventsource.Event {
	data, err := json.Marshal(fields)
	if err != nil {
		panic(err)
	}
	return eventsource.Event{Standard: "fair_chance", Name: name, Coin: coinID, Block: block, Data: data}
}

func tokens(whole string) string { return whole + strings.Repeat("0", 18) }

func TestEachEventTypeGivesOneSentenceFromTheReceiptTemplates(t *testing.T) {
	tests := []struct {
		name string
		ev   eventsource.Event
		want string
	}{
		{
			name: "partial fill",
			ev: event("fc_sell", 500, map[string]any{
				"account": "bigfish.testnet", "class": "large",
				"tokens_asked": tokens("30000000"), "tokens_sold": tokens("7100000"),
				"near_out": "1000000000000000000000000", "fill": "partial", "limit_reason": "allowance",
				"allowance_left": tokens("7100000"), "window_end_ns": at1502,
			}),
			want: "bigfish.testnet tried to sell 30.0M MOTH (3.0% of supply). The rule filled 7.1M (0.7%). It can sell up to 7.1M more after 15:02.",
		},
		{
			name: "cap full",
			ev: event("fc_cap_filled", 501, map[string]any{
				"window_start_ns": "1794578520000000000", "window_end_ns": at1502,
				"real_near": "40000000000000000000000000", "cap_total": "2130000000000000000000000",
				"cap_used": "2130000000000000000000000", "by_account": "bigfish.testnet", "fill_index": 1,
			}),
			want: "Pool cap full for this hour: 2.13 NEAR out. Limited sells resume at 15:02.",
		},
		{
			name: "became large",
			ev: event("fc_became_large", 502, map[string]any{
				"account": "bigfish.testnet", "balance": tokens("34000000"), "large_threshold": tokens("20000000"),
			}),
			want: "bigfish.testnet now holds 3.4% of supply. It is large and cannot send tokens.",
		},
		{
			name: "creator group buy",
			ev: event("fc_buy", 503, map[string]any{
				"account": "wick.lamp.testnet", "near_in": "1", "tokens_out": tokens("4000000"),
				"marked_tokens": tokens("4000000"), "class_after": "band",
			}),
			want: "wick.lamp.testnet (creator group) bought 4.0M MOTH. These tokens count as early supply.",
		},
		{
			name: "early buy",
			ev: event("fc_buy", 104, map[string]any{
				"account": "dart.testnet", "near_in": "1", "tokens_out": tokens("9200000"),
				"marked_tokens": tokens("9200000"), "class_after": "band",
			}),
			want: "dart.testnet bought 9.2M MOTH in block 4. These tokens count as early supply.",
		},
		{
			name: "send into the creator group",
			ev: event("fc_send", 504, map[string]any{
				"from": "dart.testnet", "to": "lamp.testnet", "amount": tokens("1500000"),
				"marked_amount": "0", "to_marked_amount": tokens("1500000"),
			}),
			want: "lamp.testnet (creator group) received 1.5M MOTH. These tokens count as early supply.",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := testWriter().Sentence(tt.ev)
			if !ok || got != tt.want {
				t.Fatalf("got %q (ok %v)\nwant %q", got, ok, tt.want)
			}
		})
	}
}

func TestEventsThatChangeNoOnesRightToSellGiveNoAlert(t *testing.T) {
	for _, ev := range []eventsource.Event{
		event("fc_buy", 600, map[string]any{"account": "dart.testnet", "tokens_out": tokens("5"), "marked_tokens": "0"}),
		event("fc_send", 601, map[string]any{"from": "a.testnet", "to": "b.testnet", "amount": tokens("5"), "to_marked_amount": "0"}),
		event("fc_creator_claim", 602, map[string]any{"creator": "wick.lamp.testnet", "amount": "1"}),
	} {
		if text, ok := testWriter().Sentence(ev); ok {
			t.Errorf("%s gave an alert: %q", ev.Name, text)
		}
	}
}

func TestAPartialFillThatLeavesNothingMoreToSellInTheWindowStillNamesTheReset(t *testing.T) {
	ev := event("fc_sell", 500, map[string]any{
		"account": "bigfish.testnet", "class": "large",
		"tokens_asked": tokens("30000000"), "tokens_sold": tokens("100000"),
		"fill": "partial", "limit_reason": "cap_full", "allowance_left": "0", "window_end_ns": at1502,
	})
	got, ok := testWriter().Sentence(ev)
	want := "bigfish.testnet tried to sell 30.0M MOTH (3.0% of supply). The rule filled 100K (0.01%). Its allowance is used up until 15:02."
	if !ok || got != want {
		t.Fatalf("got %q\nwant %q", got, want)
	}
}

func TestCreatorGroupFollowsTheDotBoundary(t *testing.T) {
	buy := func(account string) eventsource.Event {
		return event("fc_buy", 503, map[string]any{"account": account, "tokens_out": tokens("1000000"), "marked_tokens": tokens("1000000")})
	}
	w := alerts.Writer{Location: time.UTC, Rules: func(string) (alerts.CoinRules, bool) {
		return alerts.CoinRules{Creator: "alice.near", LaunchBlock: 100, EarlyBlocks: 17, Symbol: "MOTH"}, true
	}}
	for account, inGroup := range map[string]bool{"alice.near": true, "pay.alice.near": true, "malice.near": false} {
		text, _ := w.Sentence(buy(account))
		if got := strings.Contains(text, "(creator group)"); got != inGroup {
			t.Errorf("%s: in group %v, alert %q", account, got, text)
		}
	}
}

func TestNoAlertSaysWaitsQueuedOrPending(t *testing.T) {
	for _, ev := range []eventsource.Event{
		event("fc_sell", 500, map[string]any{"account": "a.testnet", "class": "large", "tokens_asked": tokens("9"), "tokens_sold": tokens("1"), "fill": "partial", "limit_reason": "allowance", "allowance_left": tokens("3"), "window_end_ns": at1502}),
		event("fc_sell", 500, map[string]any{"account": "a.testnet", "class": "large", "tokens_asked": tokens("9"), "tokens_sold": "0", "fill": "none", "limit_reason": "cap_full", "allowance_left": "0", "window_end_ns": at1502}),
		event("fc_cap_filled", 501, map[string]any{"window_end_ns": at1502, "cap_used": "1"}),
	} {
		text, _ := testWriter().Sentence(ev)
		for _, banned := range []string{"waits", "queued", "pending"} {
			if strings.Contains(strings.ToLower(text), banned) {
				t.Errorf("alert %q contains %q", text, banned)
			}
		}
	}
}

func TestALongSymbolIsCutSoTheSentenceKeepsItsFacts(t *testing.T) {
	w := alerts.Writer{Location: time.UTC, Rules: func(string) (alerts.CoinRules, bool) {
		return alerts.CoinRules{Symbol: strings.Repeat("S", 4100)}, true
	}}
	got, ok := w.Sentence(event("fc_sell", 500, map[string]any{
		"account": "bigfish.testnet", "class": "large",
		"tokens_asked": tokens("30000000"), "tokens_sold": tokens("7100000"),
		"near_out": "1000000000000000000000000", "fill": "partial", "limit_reason": "allowance",
		"allowance_left": tokens("7100000"), "window_end_ns": at1502,
	}))
	if !ok || len([]rune(got)) > 400 || !strings.Contains(got, "It can sell up to 7.1M") || !strings.Contains(got, "…") {
		t.Fatalf("got %d characters: %.200s", len([]rune(got)), got)
	}
}
