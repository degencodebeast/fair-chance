package eventsource_test

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"sync"
	"testing"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/recordedrpc"
)

// countingProxy forwards to target and counts the JSON-RPC calls by method.
func countingProxy(t *testing.T, target string) (*httptest.Server, func() map[string]int) {
	t.Helper()
	u, _ := url.Parse(target)
	proxy := httputil.NewSingleHostReverseProxy(u)
	var mu sync.Mutex
	counts := map[string]int{}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		r.Body = io.NopCloser(bytes.NewReader(raw))
		var req struct {
			Method string `json:"method"`
		}
		_ = json.Unmarshal(raw, &req)
		mu.Lock()
		counts[req.Method]++
		mu.Unlock()
		proxy.ServeHTTP(w, r)
	}))
	t.Cleanup(srv.Close)
	return srv, func() map[string]int {
		mu.Lock()
		defer mu.Unlock()
		out := map[string]int{}
		for k, v := range counts {
			out[k] = v
		}
		return out
	}
}

// A pass costs one extra block call per distinct block that holds an event receipt, and no other call.
func TestBlockScanReadsOneExtraBlockPerEventReceiptBlock(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	proxy, counts := countingProxy(t, rpc.URL)
	if _, _, err := eventsource.NewRPCScan(proxy.URL).Events(context.Background(), []string{coin}, 15); err != nil {
		t.Fatal(err)
	}
	want := map[string]int{"status": 1, "block": 14 + 2, "chunk": 3, "EXPERIMENTAL_tx_status": 2} // 14 = 13 heights and the head, 2 = blocks 22 and 28
	got := counts()
	for method, n := range want {
		if got[method] != n {
			t.Errorf("calls %v, want %v", got, want)
			break
		}
	}
}

func TestBlockScanKeepsTheInclusionCursorWhenTheReceiptBlockCannotBeRead(t *testing.T) {
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	u, _ := url.Parse(rpc.URL)
	proxy := httputil.NewSingleHostReverseProxy(u)
	flaky := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		r.Body = io.NopCloser(bytes.NewReader(raw))
		if bytes.Contains(raw, []byte(`"block_id":"`)) { // a block by hash
			w.WriteHeader(http.StatusServiceUnavailable)
			return
		}
		proxy.ServeHTTP(w, r)
	}))
	defer flaky.Close()

	evs, next, err := eventsource.NewRPCScan(flaky.URL).Events(context.Background(), []string{coin}, 15)
	if !errors.Is(err, eventsource.ErrTransient) || len(evs) != 0 || next != 21 {
		t.Fatalf("events %+v next %d err %v; want none, cursor 21 (the block of the transaction), a transient error", evs, next, err)
	}
}
