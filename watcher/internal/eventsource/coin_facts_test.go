package eventsource_test

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	"fairchance/watcher/internal/eventsource"
)

// nodeServer answers like a NEAR node: a final block, and coin views by method name.
func nodeServer(t *testing.T, views map[string]string) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string `json:"method"`
			Params struct {
				MethodName string `json:"method_name"`
			} `json:"params"`
		}
		_ = json.NewDecoder(r.Body).Decode(&req)
		switch req.Method {
		case "block":
			fmt.Fprint(w, `{"jsonrpc":"2.0","id":"fc","result":{"header":{"height":90},"chunks":[]}}`)
		case "query":
			body, _ := json.Marshal([]byte(views[req.Params.MethodName]))
			fmt.Fprintf(w, `{"jsonrpc":"2.0","id":"fc","result":{"result":%s,"logs":[]}}`, body)
		}
	}))
	t.Cleanup(srv.Close)
	return srv
}

func TestHeadCursorIsTheBlockAfterTheFinalHead(t *testing.T) {
	srv := nodeServer(t, nil)
	got, err := eventsource.HeadCursor(context.Background(), srv.URL)
	if err != nil || got != 91 {
		t.Fatalf("got %d, %v; want 91", got, err)
	}
}

func TestReadCoinFactsGivesCreatorLaunchBlockEarlyBlocksAndSymbol(t *testing.T) {
	srv := nodeServer(t, map[string]string{
		"fc_rules":    `{"creator":"wick.lamp.testnet","launch_block":100,"early_blocks":17}`,
		"ft_metadata": `{"spec":"ft-1.0.0","name":"Moth Lamp","symbol":"MOTH","decimals":18}`,
	})
	got, err := eventsource.ReadCoinFacts(context.Background(), srv.URL, "moth.factory.testnet")
	want := eventsource.CoinFacts{Creator: "wick.lamp.testnet", LaunchBlock: 100, EarlyBlocks: 17, Symbol: "MOTH"}
	if err != nil || got != want {
		t.Fatalf("got %+v, %v; want %+v", got, err, want)
	}
}
