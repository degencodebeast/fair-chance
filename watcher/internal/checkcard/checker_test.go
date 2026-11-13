package checkcard_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"fairchance/watcher/internal/checkcard"
)

// A local stand-in for a NEAR node: it answers view calls from a fixture file, by method name.
func TestCheckerReadsEachViewOnceAndWritesTheCard(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join("testdata", "normal_cap_full.views.json"))
	if err != nil {
		t.Fatal(err)
	}
	var byMethod map[string]json.RawMessage
	if err := json.Unmarshal(raw, &byMethod); err != nil {
		t.Fatal(err)
	}
	var asked []string
	node := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Params struct {
				RequestType string `json:"request_type"`
				AccountID   string `json:"account_id"`
				MethodName  string `json:"method_name"`
			} `json:"params"`
		}
		_ = json.NewDecoder(r.Body).Decode(&req)
		asked = append(asked, req.Params.RequestType+":"+req.Params.AccountID+":"+req.Params.MethodName)
		body, _ := json.Marshal([]byte(byMethod[req.Params.MethodName]))
		_, _ = w.Write([]byte(`{"jsonrpc":"2.0","id":"fc","result":{"result":` + string(body) + `,"logs":[]}}`))
	}))
	defer node.Close()

	checker := checkcard.Checker{RPCURL: node.URL, Now: func() time.Time { return utc(2026, 11, 13, 14, 24) }, Location: time.UTC}
	got, err := checker.Check(context.Background(), "moth.factory.testnet")
	if err != nil {
		t.Fatal(err)
	}
	want, _ := os.ReadFile(filepath.Join("testdata", "normal_cap_full.golden.txt"))
	if got != strings.TrimSuffix(string(want), "\n") {
		t.Fatalf("got:\n%s", got)
	}
	if len(asked) != 5 {
		t.Fatalf("asked %v", asked)
	}
	for _, a := range asked {
		if !strings.HasPrefix(a, "call_function:moth.factory.testnet:") {
			t.Errorf("not a view call on the coin: %s", a)
		}
	}
}
