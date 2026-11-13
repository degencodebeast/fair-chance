package eventsource_test

import (
	"context"
	"strings"
	"testing"

	"fairchance/watcher/internal/eventsource"
)

// The URL below is fake. Port 1 refuses the connection, so the request fails with a *url.Error.
const fakeKeyedURL = "https://127.0.0.1:1/SECRET-TOKEN/"

func TestRPCErrorDoesNotHoldTheURLToken(t *testing.T) {
	_, err := eventsource.ReadPreBuyViews(context.Background(), fakeKeyedURL, "bag.factory.testnet")
	if err == nil {
		t.Fatal("want an error")
	}
	if strings.Contains(err.Error(), "SECRET-TOKEN") || strings.Contains(err.Error(), "127.0.0.1") {
		t.Fatalf("error holds the URL: %v", err)
	}
}

func TestFastNearErrorDoesNotHoldTheURLToken(t *testing.T) {
	f := eventsource.NewFastNear("https://127.0.0.1:1/SECRET-TOKEN")
	_, _, err := f.Events(context.Background(), []string{"bag.factory.testnet"}, eventsource.Cursor(100))
	if err == nil {
		t.Fatal("want an error")
	}
	if strings.Contains(err.Error(), "SECRET-TOKEN") || strings.Contains(err.Error(), "127.0.0.1") {
		t.Fatalf("error holds the URL: %v", err)
	}
}
