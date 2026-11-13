package alerts_test

import (
	"context"
	"encoding/json"
	"errors"
	"testing"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

// fixedSource returns the events at or after the cursor, then a cursor past the last block.
type fixedSource struct {
	events []eventsource.Event
	next   eventsource.Cursor
}

func (s fixedSource) Events(_ context.Context, _ []string, since eventsource.Cursor) ([]eventsource.Event, eventsource.Cursor, error) {
	var out []eventsource.Event
	for _, ev := range s.events {
		if eventsource.Cursor(ev.Block) >= since {
			out = append(out, ev)
		}
	}
	return out, s.next, nil
}

// failOnce fails the call with the given number (1-based), then works.
type failOnce struct {
	telegram.Fake
	call, failAt int
}

func (f *failOnce) Send(ctx context.Context, chatID int64, text string) error {
	f.call++
	if f.call == f.failAt {
		return errors.New("send failed")
	}
	return f.Fake.Send(ctx, chatID, text)
}

func sellAt(block uint64, account string) eventsource.Event {
	data, _ := json.Marshal(map[string]string{
		"account": account, "class": "small", "tokens_asked": "1", "tokens_sold": "1",
		"near_out": "1", "fill": "full",
	})
	return eventsource.Event{Standard: "fair_chance", Name: "fc_sell", Block: block, Data: data}
}

func TestAFailedSendKeepsTheCursorAtTheFirstUnsentEventsBlock(t *testing.T) {
	src := fixedSource{events: []eventsource.Event{sellAt(20, "a.test"), sellAt(25, "b.test")}, next: 30}
	sender := &failOnce{failAt: 2}

	sent, next, err := alerts.SendNew(context.Background(), src, []string{"coin"}, 15, sender, 1)
	if err == nil || sent != 1 || next != 25 {
		t.Fatalf("first pass: sent %d next %d err %v; want 1, 25, an error", sent, next, err)
	}

	sent, next, err = alerts.SendNew(context.Background(), src, []string{"coin"}, next, sender, 1)
	if err != nil || sent != 1 || next != 30 {
		t.Fatalf("second pass: sent %d next %d err %v; want 1, 30, nil", sent, next, err)
	}
	if len(sender.Sent) != 2 || sender.Sent[1].Text[:6] != "b.test" {
		t.Fatalf("sent %+v", sender.Sent)
	}
}
