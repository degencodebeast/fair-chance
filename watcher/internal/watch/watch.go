// Package watch runs one read pass: new events from the source become alerts at the sender.
package watch

import (
	"context"

	"fairchance/watcher/internal/alert"
	"fairchance/watcher/internal/events"
	"fairchance/watcher/internal/telegram"
)

// Pass reads the events since the cursor and sends one alert per event that has one.
// It returns the number of alerts sent and the next cursor.
func Pass(ctx context.Context, src events.Source, coins []string, since events.Cursor, to telegram.Sender, chatID int64) (int, events.Cursor, error) {
	evs, next, err := src.Events(ctx, coins, since)
	sent := 0
	for _, ev := range evs {
		text, ok := alert.Sentence(ev)
		if !ok {
			continue
		}
		if err := to.Send(ctx, chatID, text); err != nil {
			return sent, next, err
		}
		sent++
	}
	return sent, next, err
}
