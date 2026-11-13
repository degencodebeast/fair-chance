package alerts_test

import (
	"context"
	"testing"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/recordedrpc"
	"fairchance/watcher/internal/telegram"
)

const coin = "walk.factory.sandbox"

func TestOneRecordedSellBecomesOneAlertAtTheFakeSender(t *testing.T) {
	ctx := context.Background()
	rpc := recordedrpc.Serve(t, "../../../testdata/recorded-rpc-walking-path", 27)
	cursor, err := eventsource.LaunchCursor(ctx, rpc.URL, coin)
	if err != nil {
		t.Fatal(err)
	}
	fake := &telegram.Fake{}

	sent, next, err := alerts.SendNew(ctx, eventsource.NewRPCScan(rpc.URL), []string{coin}, cursor, alerts.Writer{}, fake, 42)
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
