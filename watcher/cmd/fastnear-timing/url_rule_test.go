package main

import (
	"context"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

// No URL in the report's stopped_by: a closed RPC gives a net/http error that holds its URL.
func TestStoppedByHoldsNoURL(t *testing.T) {
	closed := httptest.NewServer(nil)
	url := closed.URL + "/FAKEKEY0123456789"
	closed.Close()
	r := report{Coin: coin, RPCURL: url, LaunchBlock: 100}
	r.Events = append(r.Events, row{Seen: tSeen(0)})
	back := limitedBackOff(time.Millisecond)
	_ = finishLimited(context.Background(), &r, &back)
	// net/http also names the host and port it dialed ("dial tcp 127.0.0.1:…"); a bare host holds no
	// key. The URL itself, with its path, must be gone.
	if r.StoppedBy == "" || strings.Contains(r.StoppedBy, "FAKEKEY") || strings.Contains(r.StoppedBy, "://") {
		t.Fatalf("stopped_by: %q", r.StoppedBy)
	}
}

func TestTheTimingToolRefusesAURLWithAQuery(t *testing.T) {
	if err := checkEndpoints("https://rpc.testnet.invalid/v3?key=FAKEKEY0123456789", "https://tx.test.fastnear.com"); err == nil || strings.Contains(err.Error(), "FAKEKEY") {
		t.Fatalf("%v", err)
	}
	if err := checkEndpoints("https://rpc.testnet.near.org", "https://tx.test.fastnear.com"); err != nil {
		t.Fatal(err)
	}
	if err := checkEndpoints("https://example-testnet.invalid/abc123def/", "https://tx.test.fastnear.com"); err != nil {
		t.Fatal(err)
	}
}
