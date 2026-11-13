package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

// Opus review round 2, finding 4 (from its repro): FastNEAR answers 429 twice, then one event;
// the RPC answers 429 to finish's first reads. One run obeys at most two waits in all, and the
// report says how many it obeyed.
func TestTheWholeToolObeysAtMostTwoWaits(t *testing.T) {
	var fn429, rpc429 int32
	rpc := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		s := string(raw)
		switch {
		case strings.Contains(s, "fc_rules"):
			fmt.Fprint(w, `{"jsonrpc":"2.0","id":"fc","result":{"result":[123,34,108,97,117,110,99,104,95,98,108,111,99,107,34,58,49,48,48,125],"logs":[]}}`)
		case strings.Contains(s, "ft_metadata"):
			fmt.Fprint(w, `{"jsonrpc":"2.0","id":"fc","result":{"result":[123,125],"logs":[]}}`)
		case atomic.AddInt32(&rpc429, 1) <= 2:
			w.WriteHeader(http.StatusTooManyRequests)
		case strings.Contains(s, "EXPERIMENTAL_tx_status"):
			fmt.Fprint(w, `{"jsonrpc":"2.0","id":"fc","result":{"receipts_outcome":[{"id":"rSell","block_hash":"bh"}]}}`)
		default:
			fmt.Fprint(w, `{"jsonrpc":"2.0","id":"fc","result":{"header":{"height":120,"timestamp_nanosec":"1000"}}}`)
		}
	}))
	defer rpc.Close()
	fn := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if atomic.AddInt32(&fn429, 1) <= 2 {
			w.WriteHeader(http.StatusTooManyRequests)
			return
		}
		switch r.URL.Path {
		case "/v0/account":
			fmt.Fprint(w, `{"account_txs":[{"transaction_hash":"txSell","tx_block_height":120}],"resume_token":null}`)
		case "/v0/transactions":
			fmt.Fprintf(w, `{"transactions":[{"transaction":{"hash":"txSell","signer_id":"b.testnet"},"receipts":[{"receipt":{"receipt_id":"rSell","receiver_id":"%s"},"execution_outcome":{"outcome":{"logs":[%s],"status":{"SuccessValue":""}}}}]}]}`, coin, ev("fc_sell"))
		}
	}))
	defer fn.Close()
	r := report{Coin: coin, RPCURL: rpc.URL, FastNearURL: fn.URL}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	back := limitedBackOff(10 * time.Millisecond)
	_ = runLimited(ctx, &r, 10*time.Millisecond, 200*time.Millisecond, 0, 10*time.Millisecond, &back)
	ferr := finishLimited(ctx, &r, &back)
	if r.RateLimitWaits != 2 {
		t.Fatalf("the run obeyed %d waits; want 2 (the rule allows 2)", r.RateLimitWaits)
	}
	if ferr == nil || !strings.Contains(r.StoppedBy, "limit") {
		t.Fatalf("finish did not stop on the third 429: %v; stopped_by %q", ferr, r.StoppedBy)
	}
}
