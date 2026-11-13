package eventsource_test

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/recordedrpc"
)

const coin = "walk.factory.sandbox"

// The launch transaction sits in a block before launch_block (the factory receipt, then the coin
// batch), so the scan starts LaunchScanMarginBlocks before it to read the opening buy.
func TestLaunchCursorStartsTheScanMarginBeforeTheLaunchBlock(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	cursor, err := eventsource.LaunchCursor(context.Background(), rpc.URL, coin)
	if err != nil {
		t.Fatal(err)
	}
	if eventsource.LaunchScanMarginBlocks != 10 {
		t.Fatalf("margin %d, want 10", eventsource.LaunchScanMarginBlocks)
	}
	if cursor != 5 {
		t.Fatalf("cursor %d, want 5 (launch block 15 minus 10)", cursor)
	}
}

func TestScanStartStopsAtBlockZero(t *testing.T) {
	for _, c := range []struct{ launch, want uint64 }{{15, 5}, {10, 0}, {3, 0}, {0, 0}} {
		if got := eventsource.ScanStart(c.launch); got != eventsource.Cursor(c.want) {
			t.Fatalf("ScanStart(%d) = %d, want %d", c.launch, got, c.want)
		}
	}
}

func TestBlockScanFromTheLaunchCursorFindsTheCoinEvents(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	cursor, err := eventsource.LaunchCursor(context.Background(), rpc.URL, coin)
	if err != nil {
		t.Fatal(err)
	}
	got, _, err := eventsource.NewRPCScan(rpc.URL).Events(context.Background(), []string{coin}, cursor)
	if err != nil || len(got) != 2 {
		t.Fatalf("got %+v err %v", got, err)
	}
}

func TestBlockScanReturnsTheCoinEventsInChainOrder(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	got, next, err := eventsource.NewRPCScan(rpc.URL).Events(context.Background(), []string{coin}, 15)
	if err != nil {
		t.Fatal(err)
	}
	if next != 28 {
		t.Fatalf("next %d, want 28", next)
	}
	if len(got) != 2 || got[0].Name != "fc_buy" || got[1].Name != "fc_sell" {
		t.Fatalf("got %+v", got)
	}
	sell := got[1]
	if sell.Coin != coin || sell.Block != 27 || sell.Version != "1.0.0-rc.4" || sell.Standard != "fair_chance" {
		t.Fatalf("sell %+v", sell)
	}
	var data struct {
		TokensSold string `json:"tokens_sold"`
	}
	if err := json.Unmarshal(sell.Data, &data); err != nil || data.TokensSold != "4950495049504950495049504" {
		t.Fatalf("data %s (%v)", sell.Data, err)
	}
}

func TestBlockScanFromTheHeadPlusOneReturnsNothing(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	got, next, err := eventsource.NewRPCScan(rpc.URL).Events(context.Background(), []string{coin}, 28)
	if err != nil || len(got) != 0 || next != 28 {
		t.Fatalf("got %v next %d err %v", got, next, err)
	}
}

func TestBlockScanIgnoresOtherCoins(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	got, _, err := eventsource.NewRPCScan(rpc.URL).Events(context.Background(), []string{"other.factory.sandbox"}, 15)
	if err != nil || len(got) != 0 {
		t.Fatalf("got %v err %v", got, err)
	}
}

func TestBlockScanFailsWhenTheCursorIsBelowTheNodesEarliestBlock(t *testing.T) {
	rpc := recordedrpc.ServeWithEarliest(t, "../../../testdata/recorded-rpc-walking-path", 27, 20)
	got, next, err := eventsource.NewRPCScan(rpc.URL).Events(context.Background(), []string{coin}, 15)
	if !errors.Is(err, eventsource.ErrBlocksGone) {
		t.Fatalf("err %v, want ErrBlocksGone", err)
	}
	if len(got) != 0 || next != 15 {
		t.Fatalf("got %v next %d; want no events and the same cursor", got, next)
	}
}
