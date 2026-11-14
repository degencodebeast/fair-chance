// Package commands answers the bot's chat commands. Only the configured chat can use /watch. Any chat can
// use /start, /help and, when Public is set, a rate limited /check of coins of the one factory.
package commands

import (
	"context"
	"regexp"
	"strings"
	"time"

	"fairchance/watcher/internal/telegram"
)

// Watcher starts alerts for one coin account.
type Watcher interface {
	Watch(ctx context.Context, coin string) error
}

// Checker writes the pre-buy card for one coin account. It reads views only.
type Checker interface {
	Check(ctx context.Context, coin string) (string, error)
}

// Updates is where new chat messages come from (the Telegram long poll).
type Updates interface {
	Updates(ctx context.Context, offset int64) ([]telegram.Incoming, int64, error)
}

type Bot struct {
	ChatID  int64 // the one chat the bot answers
	Reply   telegram.Sender
	Watcher Watcher
	Checker Checker
	Public  *PublicCheck // nil: the bot answers the configured chat only
}

const retryAfterFailure = 3 * time.Second

var nearAccount = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{1,63}$`)

// Handle answers one message. A message from any chat but the configured one gets no reply and starts
// nothing, except /start, /help and a public /check when Public is set.
func (b Bot) Handle(ctx context.Context, msg telegram.Incoming) {
	fields := strings.Fields(msg.Text)
	if len(fields) == 0 {
		return
	}
	command, _, _ := strings.Cut(fields[0], "@") // /watch@bot_name in a group chat
	own := msg.ChatID == b.ChatID
	switch {
	case command == "/watch" && own:
		b.handleWatch(ctx, fields)
	case command == "/check" && own:
		b.handleCheck(ctx, msg.ChatID, fields)
	case command == "/check" && b.Public != nil:
		b.handlePublicCheck(ctx, msg.ChatID, fields)
	case (command == "/start" || command == "/help") && b.Public != nil:
		if own || b.Public.takeHelpSlot(msg.ChatID) {
			b.reply(ctx, msg.ChatID, b.Public.helpLine())
		}
	}
}

func (b Bot) handleWatch(ctx context.Context, fields []string) {
	if len(fields) != 2 || !nearAccount.MatchString(fields[1]) {
		b.reply(ctx, b.ChatID, "Send /watch and one coin account, for example /watch moth.factory.testnet.")
		return
	}
	coin := fields[1]
	if err := b.Watcher.Watch(ctx, coin); err != nil {
		b.reply(ctx, b.ChatID, "I cannot read "+coin+". Check the account name and try again.")
		return
	}
	b.reply(ctx, b.ChatID, "Watching "+coin+". You get one message each time a limit acts on that coin.")
}

func (b Bot) handleCheck(ctx context.Context, chat int64, fields []string) {
	if b.Checker == nil {
		return
	}
	if len(fields) != 2 || !nearAccount.MatchString(fields[1]) {
		b.reply(ctx, chat, "Send /check and one coin account, for example /check moth.factory.testnet.")
		return
	}
	coin := fields[1]
	card, err := b.Checker.Check(ctx, coin)
	if err != nil {
		b.reply(ctx, b.ChatID, "I cannot read "+coin+". Check the account name and try again.")
		return
	}
	b.reply(ctx, chat, card)
}

func (b Bot) reply(ctx context.Context, chat int64, text string) {
	_ = b.Reply.Send(ctx, chat, text)
}

// Serve reads messages from the long poll and handles each one until the context ends.
func (b Bot) Serve(ctx context.Context, updates Updates) error {
	var offset int64
	for {
		msgs, next, err := updates.Updates(ctx, offset)
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if err != nil {
			// A failed poll is retried at the same offset, so no message is lost.
			select {
			case <-ctx.Done():
				return ctx.Err()
			case <-time.After(retryAfterFailure):
			}
			continue
		}
		offset = next
		for _, m := range msgs {
			b.Handle(ctx, m)
		}
	}
}
