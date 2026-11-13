// Package telegram sends alert text to a chat. Tests and the walking path use Fake.
package telegram

import (
	"context"
	"sync"
)

type Sender interface {
	Send(ctx context.Context, chatID int64, text string) error
}

type Message struct {
	ChatID int64  `json:"chat_id"`
	Text   string `json:"text"`
}

// Fake keeps every message it is given, in order, and sends nothing.
type Fake struct {
	mu   sync.Mutex
	Sent []Message
}

func (f *Fake) Send(_ context.Context, chatID int64, text string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.Sent = append(f.Sent, Message{ChatID: chatID, Text: text})
	return nil
}
