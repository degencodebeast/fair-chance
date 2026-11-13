package watch_test

import (
	"context"
	"testing"

	"fairchance/watcher/internal/events"
	"fairchance/watcher/internal/rpcfake"
	"fairchance/watcher/internal/telegram"
	"fairchance/watcher/internal/watch"
)

const coin = "walk.factory.sandbox"

func TestOneRecordedSellBecomesOneAlertAtTheFakeSender(t *testing.T) {
	ctx := context.Background()
	rpc := rpcfake.Serve(t, "../../../testdata/rpc-sandbox-walk", 27)
	cursor, err := events.LaunchCursor(ctx, rpc.URL, coin)
	if err != nil {
		t.Fatal(err)
	}
	fake := &telegram.Fake{}

	sent, next, err := watch.Pass(ctx, events.NewRPCScan(rpc.URL), []string{coin}, cursor, fake, 42)
	if err != nil {
		t.Fatal(err)
	}

	if sent != 1 || len(fake.Sent) != 1 {
		t.Fatalf("sent %d alerts, fake got %v", sent, fake.Sent)
	}
	want := "alice.sandbox (small holder) sold 4,950,495.04 tokens for 0.5024 NEAR. The whole sell filled."
	if fake.Sent[0].Text != want || fake.Sent[0].ChatID != 42 {
		t.Fatalf("got %+v\nwant text %q", fake.Sent[0], want)
	}
	if next != 28 {
		t.Fatalf("next cursor %d, want 28", next)
	}
}
