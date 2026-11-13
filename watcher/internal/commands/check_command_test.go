package commands_test

import (
	"context"
	"errors"
	"testing"

	"fairchance/watcher/internal/commands"
	"fairchance/watcher/internal/telegram"
)

type fixedChecker struct {
	text    string
	err     error
	checked []string
}

func (f *fixedChecker) Check(_ context.Context, coin string) (string, error) {
	f.checked = append(f.checked, coin)
	return f.text, f.err
}

func newCheckBot(c *fixedChecker) (commands.Bot, *telegram.Fake) {
	fake := &telegram.Fake{}
	return commands.Bot{ChatID: builderChat, Reply: fake, Watcher: &recordingWatcher{}, Checker: c}, fake
}

func TestCheckRepliesWithTheCardInTheConfiguredChat(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, fake := newCheckBot(checker)
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/check moth.factory.testnet"})

	if len(checker.checked) != 1 || checker.checked[0] != "moth.factory.testnet" {
		t.Fatalf("checked %v", checker.checked)
	}
	if len(fake.Sent) != 1 || fake.Sent[0].Text != "card text" || fake.Sent[0].ChatID != builderChat {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestCheckFromAnotherChatReadsNothingAndGetsNoReply(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, fake := newCheckBot(checker)
	bot.Handle(context.Background(), telegram.Incoming{ChatID: 999, Text: "/check moth.factory.testnet"})
	if len(checker.checked) != 0 || len(fake.Sent) != 0 {
		t.Fatalf("checked %v sent %+v", checker.checked, fake.Sent)
	}
}

func TestCheckWithABadOrMissingAccountAsksAgainAndReadsNothing(t *testing.T) {
	for _, text := range []string{"/check", "/check not an account", "/check a.testnet extra"} {
		checker := &fixedChecker{}
		bot, fake := newCheckBot(checker)
		bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: text})
		want := "Send /check and one coin account, for example /check moth.factory.testnet."
		if len(checker.checked) != 0 || len(fake.Sent) != 1 || fake.Sent[0].Text != want {
			t.Errorf("%q: checked %v sent %+v", text, checker.checked, fake.Sent)
		}
	}
}

func TestCheckOfACoinThatCannotBeReadSaysSo(t *testing.T) {
	bot, fake := newCheckBot(&fixedChecker{err: errors.New("rpc down")})
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/check moth.factory.testnet"})
	want := "I cannot read moth.factory.testnet. Check the account name and try again."
	if len(fake.Sent) != 1 || fake.Sent[0].Text != want {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestCheckAcceptsTheBotNameSuffixInAGroupChat(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _ := newCheckBot(checker)
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/check@fair_chance_bot moth.factory.testnet"})
	if len(checker.checked) != 1 {
		t.Fatalf("checked %v", checker.checked)
	}
}
