package telegram

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"time"
)

const (
	defaultBotAPIURL = "https://api.telegram.org"
	tokenVariable    = "TELEGRAM_BOT_TOKEN"
	longPollSeconds  = 25
)

// Incoming is one text message sent to the bot.
type Incoming struct {
	ChatID int64
	Text   string
}

// BotAPI is the real Telegram client: it sends with sendMessage and reads with getUpdates (long polling).
// It opens no port.
type BotAPI struct {
	token   string
	baseURL string
	http    *http.Client
}

func NewBotAPI(token, baseURL string) *BotAPI {
	if baseURL == "" {
		baseURL = defaultBotAPIURL
	}
	return &BotAPI{token: token, baseURL: baseURL, http: &http.Client{Timeout: (longPollSeconds + 15) * time.Second}}
}

// NewBotAPIFromEnvironment reads the bot token from the environment. Only the watcher command calls it.
func NewBotAPIFromEnvironment() (*BotAPI, error) {
	token := os.Getenv(tokenVariable)
	if token == "" {
		return nil, fmt.Errorf("set %s", tokenVariable)
	}
	return NewBotAPI(token, ""), nil
}

func (b *BotAPI) Send(ctx context.Context, chatID int64, text string) error {
	return b.call(ctx, "sendMessage", map[string]any{"chat_id": chatID, "text": Prepare(text)}, nil)
}

// Updates waits for new messages from the offset on. It returns the text messages and the next offset.
func (b *BotAPI) Updates(ctx context.Context, offset int64) ([]Incoming, int64, error) {
	var res []struct {
		UpdateID int64 `json:"update_id"`
		Message  *struct {
			Chat struct {
				ID int64 `json:"id"`
			} `json:"chat"`
			Text string `json:"text"`
		} `json:"message"`
	}
	if err := b.call(ctx, "getUpdates", map[string]any{"offset": offset, "timeout": longPollSeconds}, &res); err != nil {
		return nil, offset, err
	}
	var out []Incoming
	next := offset
	for _, u := range res {
		next = max(next, u.UpdateID+1)
		if u.Message != nil && u.Message.Text != "" {
			out = append(out, Incoming{ChatID: u.Message.Chat.ID, Text: u.Message.Text})
		}
	}
	return out, next, nil
}

func (b *BotAPI) call(ctx context.Context, method string, body any, result any) error {
	raw, err := json.Marshal(body)
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, b.baseURL+"/bot"+b.token+"/"+method, bytes.NewReader(raw))
	if err != nil {
		return fmt.Errorf("telegram %s: bad request", method)
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := b.http.Do(req)
	if err != nil {
		// The error text of net/http holds the URL, and the URL holds the token. Keep the cause only.
		var urlErr *url.Error
		if errors.As(err, &urlErr) {
			err = urlErr.Err
		}
		return fmt.Errorf("telegram %s: %w", method, err)
	}
	defer resp.Body.Close()
	var envelope struct {
		OK          bool            `json:"ok"`
		Description string          `json:"description"`
		Result      json.RawMessage `json:"result"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&envelope); err != nil {
		return fmt.Errorf("telegram %s: HTTP %d", method, resp.StatusCode)
	}
	if !envelope.OK {
		return fmt.Errorf("telegram %s: HTTP %d: %s", method, resp.StatusCode, envelope.Description)
	}
	if result != nil {
		return json.Unmarshal(envelope.Result, result)
	}
	return nil
}
