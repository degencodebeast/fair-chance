// Package eventsource reads fair_chance events for a set of coins. The watcher sees events only
// through Source, so the network adapter can change without touching alerts or commands.
package eventsource

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
)

// Event is one NEP-297 event log from a successful receipt of a watched coin.
type Event struct {
	Standard  string          // "fair_chance" or "fair_chance_factory"
	Version   string          // interface version, for example "1.0.0-rc.4"
	Name      string          // event name, for example "fc_sell"
	Coin      string          // account that executed the receipt
	ReceiptID string          // receipt that logged the event
	Block     uint64          // height of the block that included the transaction
	Data      json.RawMessage // the single object in data[0]
}

// Cursor is the next block height to read.
type Cursor uint64

// Source gives new fair_chance and fair_chance_factory events for these coins since a cursor.
// Events come in chain order. The returned cursor is the one to pass next time.
type Source interface {
	Events(ctx context.Context, coins []string, since Cursor) ([]Event, Cursor, error)
}

// Standards the watcher reads. Token events (nep141) are not part of the coin interface.
var standards = map[string]bool{"fair_chance": true, "fair_chance_factory": true}

// parseLog returns the event in one log line, or false when the line is not a watched event.
func parseLog(line string) (Event, bool) {
	body, ok := strings.CutPrefix(line, "EVENT_JSON:")
	if !ok {
		return Event{}, false
	}
	var raw struct {
		Standard string            `json:"standard"`
		Version  string            `json:"version"`
		Event    string            `json:"event"`
		Data     []json.RawMessage `json:"data"`
	}
	if err := json.Unmarshal([]byte(body), &raw); err != nil || !standards[raw.Standard] || len(raw.Data) == 0 {
		return Event{}, false
	}
	return Event{Standard: raw.Standard, Version: raw.Version, Name: raw.Event, Data: raw.Data[0]}, true
}

// LaunchCursor is the first cursor for a coin: its launch block, from the coin's fc_rules view.
func LaunchCursor(ctx context.Context, rpcURL, coin string) (Cursor, error) {
	var rules struct {
		LaunchBlock uint64 `json:"launch_block"`
	}
	if err := newRPC(rpcURL).view(ctx, coin, "fc_rules", &rules); err != nil {
		return 0, fmt.Errorf("fc_rules of %s: %w", coin, err)
	}
	return Cursor(rules.LaunchBlock), nil
}
