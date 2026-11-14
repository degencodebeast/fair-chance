// Package recordedrpc serves recorded NEAR JSON-RPC responses from a folder, for tests.
//
// Files: block_<height>.json, chunk_<hash>.json, tx_<hash>.json, query_<method>.json, status.json.
// A block request by finality answers with block_<head>.json. A missing block answers
// UNKNOWN_BLOCK the way a node does for a height with no block.
package recordedrpc

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

type request struct {
	Method string `json:"method"`
	Params struct {
		BlockID    json.RawMessage `json:"block_id"` // a height (number) or a block hash (string)
		Finality   string          `json:"finality"`
		ChunkID    string          `json:"chunk_id"`
		TxHash     string          `json:"tx_hash"`
		MethodName string          `json:"method_name"`
	} `json:"params"`
}

func Serve(t testing.TB, dir string, head uint64) *httptest.Server {
	t.Helper()
	return serve(t, dir, head, nil)
}

// ServeWithEarliest is Serve, but the status answer gives this earliest block height, as a node
// that dropped older blocks does.
func ServeWithEarliest(t testing.TB, dir string, head, earliest uint64) *httptest.Server {
	t.Helper()
	return serve(t, dir, head, &earliest)
}

func serve(t testing.TB, dir string, head uint64, earliest *uint64) *httptest.Server {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req request
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		var name string
		switch req.Method {
		case "block":
			height := head
			var hash string
			if json.Unmarshal(req.Params.BlockID, &hash) == nil && hash != "" {
				name = blockFileByHash(dir, hash)
				if name == "" {
					http.Error(w, "not recorded: block "+hash, http.StatusNotFound)
					return
				}
				break
			}
			var id uint64
			if json.Unmarshal(req.Params.BlockID, &id) == nil {
				height = id
			}
			name = fmt.Sprintf("block_%d.json", height)
			if _, err := os.Stat(filepath.Join(dir, name)); err != nil {
				fmt.Fprintf(w, `{"jsonrpc":"2.0","id":"fc","error":{"name":"HANDLER_ERROR","cause":{"info":{},"name":"UNKNOWN_BLOCK"},"code":-32000,"message":"Server error","data":"DB Not Found Error: BLOCK HEIGHT: %d \n Cause: Unknown"}}`, height)
				return
			}
		case "chunk":
			name = "chunk_" + req.Params.ChunkID + ".json"
		case "EXPERIMENTAL_tx_status":
			name = "tx_" + req.Params.TxHash + ".json"
		case "status":
			if earliest != nil {
				fmt.Fprintf(w, `{"jsonrpc":"2.0","id":"fc","result":{"sync_info":{"earliest_block_height":%d,"latest_block_height":%d}}}`, *earliest, head)
				return
			}
			name = "status.json"
		case "query":
			name = "query_" + req.Params.MethodName + ".json"
		default:
			http.Error(w, "method not recorded: "+req.Method, http.StatusNotFound)
			return
		}
		body, err := os.ReadFile(filepath.Join(dir, name))
		if err != nil {
			http.Error(w, "not recorded: "+name, http.StatusNotFound)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(body)
	}))
	t.Cleanup(srv.Close)
	return srv
}

// blockFileByHash finds the recorded block file whose header holds this hash, or "".
func blockFileByHash(dir, hash string) string {
	files, _ := filepath.Glob(filepath.Join(dir, "block_*.json"))
	for _, f := range files {
		raw, err := os.ReadFile(f)
		if err != nil {
			continue
		}
		var doc struct {
			Result struct {
				Header struct {
					Hash string `json:"hash"`
				} `json:"header"`
			} `json:"result"`
		}
		if json.Unmarshal(raw, &doc) == nil && doc.Result.Header.Hash == hash {
			return filepath.Base(f)
		}
	}
	return ""
}
