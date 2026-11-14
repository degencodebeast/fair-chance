package alerts

import (
	"context"

	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

// SendNew reads the events since the cursor and sends one alert per event that has one.
// It returns the number of alerts sent and the next cursor.
//
// When a send fails, the next cursor is the lowest inclusion block among the event whose alert
// failed and the events after it, so the next pass sends them again. Alerts already sent can then
// be sent twice; a lost alert is worse than a repeated one.
func SendNew(ctx context.Context, src eventsource.Source, coins []string, since eventsource.Cursor, write Writer, to telegram.Sender, chatID int64) (int, eventsource.Cursor, error) {
	evs, next, err := src.Events(ctx, coins, since)
	sent := 0
	for i, ev := range evs {
		text, ok := write.Sentence(ev)
		if !ok {
			continue
		}
		if sendErr := to.Send(ctx, chatID, text); sendErr != nil {
			return sent, restartBlock(evs[i:]), sendErr
		}
		sent++
	}
	return sent, next, err
}

// restartBlock is the block to read again from after a failed send: the lowest inclusion block among
// the unsent events. The source orders events by the block where the receipt ran, and the next read
// filters by the block that included the transaction, and the two orders can differ. An event with
// no inclusion block uses its own block.
func restartBlock(unsent []eventsource.Event) eventsource.Cursor {
	from := eventsource.Cursor(^uint64(0))
	for _, ev := range unsent {
		at := ev.Included
		if at == 0 {
			at = ev.Block
		}
		from = min(from, eventsource.Cursor(at))
	}
	return from
}
