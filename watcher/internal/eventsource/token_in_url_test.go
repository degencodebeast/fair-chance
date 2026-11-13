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
	if strings.Contains(err.Error(), "SECRET-TOKEN") || strings.Contains(err.Error(), "https://") {
		t.Fatalf("error holds the URL: %v", err)
	}
}

func TestFastNearErrorDoesNotHoldTheURLToken(t *testing.T) {
	f := eventsource.NewFastNear("https://127.0.0.1:1/SECRET-TOKEN")
	_, _, err := f.Events(context.Background(), []string{"bag.factory.testnet"}, eventsource.Cursor(100))
	if err == nil {
		t.Fatal("want an error")
	}
	if strings.Contains(err.Error(), "SECRET-TOKEN") || strings.Contains(err.Error(), "https://") {
		t.Fatalf("error holds the URL: %v", err)
	}
}

// A URL that does not parse makes http.NewRequest fail with an error that holds the whole URL.
var unparsableKeyedURLs = []string{
	"https://127.0.0.1:1/SECRET-TOKEN/\n",
	"https://127.0.0.1:1/SECRET-TOKEN/\r",
	"https://127.0.0.1:1/SECRET-TOKEN/%zz",
}

func assertNoToken(t *testing.T, what string, err error) {
	t.Helper()
	if err == nil {
		t.Fatalf("%s: want an error", what)
	}
	if strings.Contains(err.Error(), "SECRET-TOKEN") || strings.Contains(err.Error(), "https://") {
		t.Fatalf("%s: error holds the URL: %q", what, err)
	}
}

func TestUnparsableURLErrorsDoNotHoldTheToken(t *testing.T) {
	ctx := context.Background()
	for _, u := range unparsableKeyedURLs {
		_, err := eventsource.ReadPreBuyViews(ctx, u, "bag.factory.testnet")
		assertNoToken(t, "ReadPreBuyViews", err)
		_, err = eventsource.HeadCursor(ctx, u)
		assertNoToken(t, "HeadCursor", err)
		_, _, err = eventsource.NewFastNear(u).Events(ctx, []string{"bag.factory.testnet"}, eventsource.Cursor(100))
		assertNoToken(t, "FastNear.Events", err)
	}
}
