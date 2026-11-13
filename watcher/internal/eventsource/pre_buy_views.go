package eventsource

import (
	"context"
	"encoding/json"
	"fmt"
)

// PreBuyViews are the coin views the /check card reads. A field that an older coin does not return is
// a nil pointer, so the card can say it is missing instead of showing a false zero.
type PreBuyViews struct {
	Name   string
	Symbol string

	LaunchNs     string // fc_rules.launch_ns
	WindowEarlyS uint64 // fc_rules.window_early_s

	LargeHolders         *uint64 // fc_pool.large_holders
	CreatorGroupShareBps *uint64 // fc_pool.creator_group_share_bps

	BlocksLeft       uint64 // fc_early_window.blocks_left
	EarlySupplyTotal string // fc_early_window.early_supply_total
	SharedAllowance  string // fc_early_window.shared_allowance

	CapWindowEndNs string // fc_cap.window_end_ns
	CapTotal       string // fc_cap.cap_total
	CapUsed        string // fc_cap.cap_used
	CapLeft        string // fc_cap.cap_left
	CapIsFull      bool   // fc_cap.is_full
}

// preBuyMethods are the view methods ParsePreBuyViews needs.
var preBuyMethods = []string{"ft_metadata", "fc_rules", "fc_pool", "fc_early_window", "fc_cap"}

// ParsePreBuyViews builds the views from each method's JSON answer, keyed by method name.
func ParsePreBuyViews(byMethod map[string]json.RawMessage) (PreBuyViews, error) {
	var metadata struct {
		Name   string `json:"name"`
		Symbol string `json:"symbol"`
	}
	var rules struct {
		LaunchNs     string `json:"launch_ns"`
		WindowEarlyS uint64 `json:"window_early_s"`
	}
	var pool struct {
		LargeHolders         *uint64 `json:"large_holders"`
		CreatorGroupShareBps *uint64 `json:"creator_group_share_bps"`
	}
	var early struct {
		BlocksLeft       uint64 `json:"blocks_left"`
		EarlySupplyTotal string `json:"early_supply_total"`
		SharedAllowance  string `json:"shared_allowance"`
	}
	var capView struct {
		WindowEndNs string `json:"window_end_ns"`
		CapTotal    string `json:"cap_total"`
		CapUsed     string `json:"cap_used"`
		CapLeft     string `json:"cap_left"`
		IsFull      bool   `json:"is_full"`
	}
	targets := map[string]any{"ft_metadata": &metadata, "fc_rules": &rules, "fc_pool": &pool, "fc_early_window": &early, "fc_cap": &capView}
	for _, method := range preBuyMethods {
		raw, ok := byMethod[method]
		if !ok {
			return PreBuyViews{}, fmt.Errorf("%s: no answer", method)
		}
		if err := json.Unmarshal(raw, targets[method]); err != nil {
			return PreBuyViews{}, fmt.Errorf("%s: %w", method, err)
		}
	}
	return PreBuyViews{
		Name: metadata.Name, Symbol: metadata.Symbol,
		LaunchNs: rules.LaunchNs, WindowEarlyS: rules.WindowEarlyS,
		LargeHolders: pool.LargeHolders, CreatorGroupShareBps: pool.CreatorGroupShareBps,
		BlocksLeft: early.BlocksLeft, EarlySupplyTotal: early.EarlySupplyTotal, SharedAllowance: early.SharedAllowance,
		CapWindowEndNs: capView.WindowEndNs, CapTotal: capView.CapTotal, CapUsed: capView.CapUsed, CapLeft: capView.CapLeft, CapIsFull: capView.IsFull,
	}, nil
}

// ReadPreBuyViews reads the views of one coin over RPC. These are view calls only.
func ReadPreBuyViews(ctx context.Context, rpcURL, coin string) (PreBuyViews, error) {
	rpc := newRPC(rpcURL)
	byMethod := map[string]json.RawMessage{}
	for _, method := range preBuyMethods {
		var raw json.RawMessage
		if err := rpc.view(ctx, coin, method, &raw); err != nil {
			return PreBuyViews{}, fmt.Errorf("%s of %s: %w", method, coin, err)
		}
		byMethod[method] = raw
	}
	return ParsePreBuyViews(byMethod)
}
