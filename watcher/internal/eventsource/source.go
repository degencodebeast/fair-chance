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
	Block     uint64          // height of the block where the receipt ran (the block of the transaction when the source cannot tell)
	Included  uint64          // height of the block that included the transaction: where a re-read must start
	TxHash    string          // the transaction that led to the receipt
	TxSigner  string          // its signer (the RPC tx status call needs it)
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

// LaunchScanMarginBlocks is how many blocks before launch_block a coin's scan starts. The launch
// transaction is included before the coin's new runs (the factory receipt, then the coin batch),
// so a scan from launch_block would miss the opening buy's events. The coin filter drops anything
// older. The web page (LAUNCH_SCAN_MARGIN_BLOCKS) and the sandbox margin test use the same value.
const LaunchScanMarginBlocks = 10

// ScanStart is the first block to scan for a coin launched at launchBlock: LaunchScanMarginBlocks
// before it, and never below block 0.
func ScanStart(launchBlock uint64) Cursor {
	if launchBlock < LaunchScanMarginBlocks {
		return 0
	}
	return Cursor(launchBlock - LaunchScanMarginBlocks)
}

// LaunchCursor is the first cursor for a coin: ScanStart of its launch block, from fc_rules. A
// node that dropped the blocks inside the margin but still has the launch block starts at its
// earliest block instead. When the node dropped the launch block too, the cursor stays at
// ScanStart, and the scan fails with ErrBlocksGone.
func LaunchCursor(ctx context.Context, rpcURL, coin string) (Cursor, error) {
	rpc := newRPC(rpcURL)
	var rules struct {
		LaunchBlock uint64 `json:"launch_block"`
	}
	if err := rpc.view(ctx, coin, "fc_rules", &rules); err != nil {
		return 0, fmt.Errorf("fc_rules of %s: %w", coin, err)
	}
	var status struct {
		SyncInfo struct {
			EarliestBlockHeight uint64 `json:"earliest_block_height"`
		} `json:"sync_info"`
	}
	if err := rpc.call(ctx, "status", map[string]any{}, &status); err != nil {
		return 0, fmt.Errorf("node status: %w", err)
	}
	start, earliest := ScanStart(rules.LaunchBlock), status.SyncInfo.EarliestBlockHeight
	if earliest <= rules.LaunchBlock && uint64(start) < earliest {
		return Cursor(earliest), nil
	}
	return start, nil
}

// CoinFacts are the fixed facts about one coin that alert sentences need.
type CoinFacts struct {
	Creator     string
	LaunchBlock uint64
	EarlyBlocks uint64
	Symbol      string // from ft_metadata
}

// ReadCoinFacts reads fc_rules and ft_metadata of the coin.
func ReadCoinFacts(ctx context.Context, rpcURL, coin string) (CoinFacts, error) {
	var rules struct {
		Creator     string `json:"creator"`
		LaunchBlock uint64 `json:"launch_block"`
		EarlyBlocks uint64 `json:"early_blocks"`
	}
	rpc := newRPC(rpcURL)
	if err := rpc.view(ctx, coin, "fc_rules", &rules); err != nil {
		return CoinFacts{}, fmt.Errorf("fc_rules of %s: %w", coin, err)
	}
	var metadata struct {
		Symbol string `json:"symbol"`
	}
	if err := rpc.view(ctx, coin, "ft_metadata", &metadata); err != nil {
		return CoinFacts{}, fmt.Errorf("ft_metadata of %s: %w", coin, err)
	}
	return CoinFacts{Creator: rules.Creator, LaunchBlock: rules.LaunchBlock, EarlyBlocks: rules.EarlyBlocks, Symbol: metadata.Symbol}, nil
}

// HeadCursor is the block after the final head: a watcher that starts here sees only new events.
func HeadCursor(ctx context.Context, rpcURL string) (Cursor, error) {
	var head blockView
	if err := newRPC(rpcURL).call(ctx, "block", map[string]any{"finality": "final"}, &head); err != nil {
		return 0, fmt.Errorf("final head: %w", err)
	}
	return Cursor(head.Header.Height + 1), nil
}
