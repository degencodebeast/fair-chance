package alerts

import (
	"context"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

// SendNew reads the events since the cursor and sends one alert per event that has one.
// It returns the number of alerts sent and the next cursor.
//
// When a send fails, the next cursor is the inclusion block of the event whose alert failed, so the
// next pass sends it again. Alerts of earlier events in that same block are then sent twice;
// a lost alert is worse than a repeated one.
func SendNew(ctx context.Context, src eventsource.Source, coins []string, since eventsource.Cursor, write Writer, to telegram.Sender, chatID int64) (int, eventsource.Cursor, error) {
	evs, next, err := src.Events(ctx, coins, since)
	sent := 0
	for _, ev := range evs {
		text, ok := write.Sentence(ev)
		if !ok {
			continue
		}
		if sendErr := to.Send(ctx, chatID, text); sendErr != nil {
			// Read again from the block that included the transaction: the next read filters by it,
			// and the receipt's block can be later.
			from := ev.Included
			if from == 0 {
				from = ev.Block
			}
			return sent, eventsource.Cursor(from), sendErr
		}
		sent++
	}
	return sent, next, err
}
