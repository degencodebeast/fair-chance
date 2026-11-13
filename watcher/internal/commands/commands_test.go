package commands_test

import (
	"context"
	"errors"
	"strings"
	"testing"

	"fairchance/watcher/internal/commands"
	"fairchance/watcher/internal/telegram"
)

const builderChat = 111

type recordingWatcher struct {
	watched []string
	err     error
}

func (r *recordingWatcher) Watch(_ context.Context, coin string) error {
	if r.err != nil {
		return r.err
	}
	r.watched = append(r.watched, coin)
	return nil
}

func newBot() (commands.Bot, *recordingWatcher, *telegram.Fake) {
	w, fake := &recordingWatcher{}, &telegram.Fake{}
	return commands.Bot{ChatID: builderChat, Reply: fake, Watcher: w}, w, fake
}

func TestWatchStartsWatchingTheCoinAndReplies(t *testing.T) {
	bot, w, fake := newBot()
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/watch moth.factory.testnet"})

	if len(w.watched) != 1 || w.watched[0] != "moth.factory.testnet" {
		t.Fatalf("watched %v", w.watched)
	}
	want := "Watching moth.factory.testnet. You get one message each time a limit acts on that coin."
	if len(fake.Sent) != 1 || fake.Sent[0].Text != want || fake.Sent[0].ChatID != builderChat {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestAMessageFromAnotherChatGetsNoReplyAndStartsNothing(t *testing.T) {
	bot, w, fake := newBot()
	for _, text := range []string{"/watch moth.factory.testnet", "/check moth.factory.testnet", "hello", "/start"} {
		bot.Handle(context.Background(), telegram.Incoming{ChatID: 999, Text: text})
	}
	if len(fake.Sent) != 0 || len(w.watched) != 0 {
		t.Fatalf("sent %+v watched %v", fake.Sent, w.watched)
	}
}

func TestWatchWithABadOrMissingAccountAsksAgain(t *testing.T) {
	for _, text := range []string{"/watch", "/watch not an account", "/watch UPPER.testnet", "/watch a.testnet extra"} {
		bot, w, fake := newBot()
		bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: text})
		if len(w.watched) != 0 || len(fake.Sent) != 1 || !strings.HasPrefix(fake.Sent[0].Text, "Send /watch and one coin account") {
			t.Errorf("%q: watched %v sent %+v", text, w.watched, fake.Sent)
		}
	}
}

func TestWatchOfACoinThatCannotBeReadSaysSo(t *testing.T) {
	bot, w, fake := newBot()
	w.err = errors.New("rpc down")
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/watch moth.factory.testnet"})
	want := "I cannot read moth.factory.testnet. Check the account name and try again."
	if len(fake.Sent) != 1 || fake.Sent[0].Text != want {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestWatchAcceptsTheBotNameSuffixOfACommandInAGroupChat(t *testing.T) {
	bot, w, _ := newBot()
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/watch@fair_chance_bot moth.factory.testnet"})
	if len(w.watched) != 1 {
		t.Fatalf("watched %v", w.watched)
	}
}

type scriptedUpdates struct {
	batches [][]telegram.Incoming
	drained chan struct{}
}

func (s *scriptedUpdates) Updates(ctx context.Context, offset int64) ([]telegram.Incoming, int64, error) {
	if len(s.batches) == 0 {
		select {
		case <-s.drained:
		default:
			close(s.drained)
		}
		<-ctx.Done()
		return nil, offset, ctx.Err()
	}
	b := s.batches[0]
	s.batches = s.batches[1:]
	return b, offset + 1, nil
}

func TestServeHandlesEveryMessageUntilTheContextEnds(t *testing.T) {
	bot, w, fake := newBot()
	ctx, cancel := context.WithCancel(context.Background())
	updates := &scriptedUpdates{drained: make(chan struct{}), batches: [][]telegram.Incoming{
		{{ChatID: 999, Text: "/watch x.testnet"}},
		{{ChatID: builderChat, Text: "/watch moth.factory.testnet"}},
	}}
	done := make(chan error)
	go func() { done <- bot.Serve(ctx, updates) }()
	<-updates.drained
	cancel()
	if err := <-done; !errors.Is(err, context.Canceled) {
		t.Fatalf("Serve ended with %v", err)
	}
	if len(w.watched) != 1 || len(fake.Sent) != 1 {
		t.Fatalf("watched %v sent %+v", w.watched, fake.Sent)
	}
}
