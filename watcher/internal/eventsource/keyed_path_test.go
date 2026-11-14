package eventsource_test

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"fairchance/watcher/internal/eventsource"
)

func assertNoKeyedPath(t *testing.T, what string, err error) {
	t.Helper()
	if err == nil {
		t.Fatalf("%s: want an error", what)
	}
	if strings.Contains(err.Error(), "abc123def") || strings.Contains(err.Error(), "://") {
		t.Fatalf("%s: error holds the URL or key: %q", what, err)
	}
}

// A server that cuts the connection gives a transport error. A server that answers 500 gives a status error.
func TestFailuresToAKeyedPathURLHoldNoKeyOrURL(t *testing.T) {
	cut := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		c, _, _ := w.(http.Hijacker).Hijack()
		c.Close()
	}))
	defer cut.Close()
	fail := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "boom "+r.URL.Path, 500)
	}))
	defer fail.Close()
	ctx := context.Background()
	for name, base := range map[string]string{"cut": cut.URL, "500": fail.URL, "closed": "http://127.0.0.1:1"} {
		u := base + "/abc123def/"
		_, err := eventsource.ReadPreBuyViews(ctx, u, "bag.factory.testnet")
		assertNoKeyedPath(t, "RPC "+name, err)
		_, _, err = eventsource.NewFastNear(u).Events(ctx, []string{"bag.factory.testnet"}, eventsource.Cursor(100))
		assertNoKeyedPath(t, "FastNear "+name, err)
	}
}
