package alerts

import (
	"context"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

// SendNew reads the events since the cursor and sends one alert per event that has one.
// It returns the number of alerts sent and the next cursor.
func SendNew(ctx context.Context, src eventsource.Source, coins []string, since eventsource.Cursor, to telegram.Sender, chatID int64) (int, eventsource.Cursor, error) {
	evs, next, err := src.Events(ctx, coins, since)
	sent := 0
	for _, ev := range evs {
		text, ok := Sentence(ev)
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
