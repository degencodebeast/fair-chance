package eventsource_test

import (
	"context"
	"encoding/json"
	"testing"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/recordedrpc"
)

const coin = "walk.factory.sandbox"

func TestLaunchCursorIsTheCoinLaunchBlock(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	cursor, err := eventsource.LaunchCursor(context.Background(), rpc.URL, coin)
	if err != nil {
		t.Fatal(err)
	}
	if cursor != 15 {
		t.Fatalf("cursor %d, want 15", cursor)
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
