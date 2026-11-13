package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

const coin = "bag.factory.fairchance.testnet"

func ev(name string) string {
	b, _ := json.Marshal(fmt.Sprintf(`EVENT_JSON:{"standard":"fair_chance","version":"1.0.0","event":"%s","data":[{}]}`, name))
	return string(b)
}

func servers(t *testing.T) (rpc, fn *httptest.Server) {
	rpc = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Params struct {
				MethodName string `json:"method_name"`
			} `json:"params"`
		}
		_ = json.NewDecoder(r.Body).Decode(&req)
		views := map[string]string{"fc_rules": `{"creator":"c.testnet","launch_block":100,"early_blocks":17}`, "ft_metadata": `{"symbol":"BAG"}`}
		body, _ := json.Marshal([]byte(views[req.Params.MethodName]))
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":"fc","result":{"result":%s,"logs":[]}}`, body)
	}))
	fn = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/v0/account":
			fmt.Fprint(w, `{"account_txs":[{"transaction_hash":"txSell","tx_block_height":120}],"resume_token":null}`)
		case "/v0/transactions":
			fmt.Fprintf(w, `{"transactions":[{"transaction":{"hash":"txSell","signer_id":"bag-buyer-01.fairchance.testnet"},"receipts":[{"receipt":{"receipt_id":"rSell","receiver_id":"%s"},"execution_outcome":{"outcome":{"logs":[%s,%s],"status":{"SuccessValue":""}}}}]}]}`, coin, ev("fc_sell"), ev("fc_cap_filled"))
		}
	}))
	t.Cleanup(rpc.Close)
	t.Cleanup(fn.Close)
	return
}

// Review finding 2: one receipt can log two events (fc_sell and fc_cap_filled); each gets a row.
func TestTwoEventsInOneReceiptGiveTwoRows(t *testing.T) {
	rpc, fn := servers(t)
	r := report{Coin: coin, RPCURL: rpc.URL, FastNearURL: fn.URL}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	err := run(ctx, &r, 10*time.Millisecond, 200*time.Millisecond, 0)
	names := []string{}
	for _, e := range r.Events {
		names = append(names, e.Event)
	}
	t.Logf("stopped: %v; events recorded: %v", err, names)
	if len(r.Events) != 2 {
		t.Fatalf("FastNEAR returned fc_sell and fc_cap_filled in receipt rSell; report kept %d: %v", len(r.Events), names)
	}
}

// Review finding 3: a 403 stops finish at once and the report says why.
func TestFinishStopsAtTheFirst403(t *testing.T) {
	calls := 0
	rpc := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		w.WriteHeader(http.StatusForbidden)
	}))
	defer rpc.Close()
	r := report{Coin: coin, RPCURL: rpc.URL, LaunchBlock: 100}
	for i := 0; i < 20; i++ {
		r.Events = append(r.Events, row{Seen: tSeen(i)})
	}
	err := finish(context.Background(), &r)
	t.Logf("RPC requests after the first 403: %d", calls-1)
	if calls > 1 {
		t.Fatalf("finish sent %d more RPC requests after a 403", calls-1)
	}
	if err == nil || r.StoppedBy == "" {
		t.Fatalf("a 403 did not stop the report: err %v, stopped_by %q", err, r.StoppedBy)
	}
}

// Review finding 3: an answer the tool does not know also stops finish at once.
func TestFinishStopsAtAnUnknownAnswer(t *testing.T) {
	calls := 0
	rpc := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		w.WriteHeader(http.StatusBadGateway)
	}))
	defer rpc.Close()
	r := report{Coin: coin, RPCURL: rpc.URL, LaunchBlock: 100}
	for i := 0; i < 5; i++ {
		r.Events = append(r.Events, row{Seen: tSeen(i)})
	}
	if err := finish(context.Background(), &r); err == nil || calls != 1 || r.StoppedBy == "" {
		t.Fatalf("err %v, calls %d, stopped_by %q", err, calls, r.StoppedBy)
	}
}

// Review finding 3: a 429 is the one answer finish waits on and reads again.
func TestFinishBacksOffOnA429AndGoesOn(t *testing.T) {
	calls := 0
	rpc := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if calls == 1 {
			w.WriteHeader(http.StatusTooManyRequests)
			return
		}
		w.WriteHeader(http.StatusForbidden)
	}))
	defer rpc.Close()
	r := report{Coin: coin, RPCURL: rpc.URL, LaunchBlock: 100}
	r.Events = append(r.Events, row{Seen: tSeen(0)})
	start := time.Now()
	err := finishWithBackOff(context.Background(), &r, time.Millisecond)
	if calls != 2 || err == nil {
		t.Fatalf("calls %d err %v: want one 429, one read again, then stop on the 403", calls, err)
	}
	if time.Since(start) > 2*time.Second {
		t.Fatal("the back-off did not use the given start")
	}
}

// Fix 2 after the first testnet run: the coin wait polls every 5 s by default and waits out a
// 429's Retry-After before it reads again.
func TestTheCoinWaitPollsEveryFiveSecondsByDefault(t *testing.T) {
	if defaultCoinWaitPoll != 5*time.Second {
		t.Fatalf("coin wait poll %s, want 5s", defaultCoinWaitPoll)
	}
}

func TestTheCoinWaitHonoursRetryAfter(t *testing.T) {
	_, fn := servers(t)
	calls := 0
	var second time.Time
	start := time.Now()
	limited := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls++
		if calls == 1 {
			w.Header().Set("Retry-After", "1")
			w.WriteHeader(http.StatusTooManyRequests)
			return
		}
		if second.IsZero() {
			second = time.Now()
		}
		var req struct {
			Params struct {
				MethodName string `json:"method_name"`
			} `json:"params"`
		}
		_ = json.NewDecoder(r.Body).Decode(&req)
		views := map[string]string{"fc_rules": `{"creator":"c.testnet","launch_block":100,"early_blocks":17}`, "ft_metadata": `{"symbol":"BAG"}`}
		body, _ := json.Marshal([]byte(views[req.Params.MethodName]))
		fmt.Fprintf(w, `{"jsonrpc":"2.0","id":"fc","result":{"result":%s,"logs":[]}}`, body)
	}))
	t.Cleanup(limited.Close)
	r := report{Coin: coin, RPCURL: limited.URL, FastNearURL: fn.URL}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_ = runWithCoinWait(ctx, &r, 10*time.Millisecond, 200*time.Millisecond, 1, 10*time.Millisecond)
	if r.LaunchBlock != 100 {
		t.Fatalf("the coin was not found: %+v", r)
	}
	if gap := second.Sub(start); gap < time.Second {
		t.Fatalf("read again %s after a 429 with Retry-After 1", gap)
	}
	if r.RateLimited != 1 {
		t.Fatalf("rate limited reads %d, want 1", r.RateLimited)
	}
}
