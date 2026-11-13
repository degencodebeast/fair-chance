package eventsource_test

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"fairchance/watcher/internal/eventsource"
)

func TestAnRPCAnswerOf429IsARateLimitError(t *testing.T) {
	node := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusTooManyRequests)
	}))
	defer node.Close()
	_, err := eventsource.ReadPreBuyViews(context.Background(), node.URL, "bag.factory.testnet")
	if !errors.Is(err, eventsource.ErrRateLimited) {
		t.Fatalf("error %v", err)
	}
	if strings.Contains(err.Error(), "FastNEAR") {
		t.Fatalf("an RPC error names another service: %v", err)
	}
}
