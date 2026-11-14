package commands_test

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"

	"fairchance/watcher/internal/telegram"
	"fairchance/watcher/internal/watchedcoins"
)

func TestWatchFromStartsTheCoinAtThatBlockAndSaysWhere(t *testing.T) {
	bot, w, fake := newBot()
	bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/watch cabal.factory.fairchance.testnet from 277635900"})

	if w.from["cabal.factory.fairchance.testnet"] != 277635900 || len(w.watched) != 0 {
		t.Fatalf("from %v watched %v", w.from, w.watched)
	}
	want := "Watching cabal.factory.fairchance.testnet from block 277635900. You get one message each time a limit acts on that coin."
	if len(fake.Sent) != 1 || fake.Sent[0].Text != want {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestWatchFromWithABadShapeAsksAgainAndStartsNothing(t *testing.T) {
	for _, text := range []string{"/watch a.testnet from", "/watch a.testnet from abc", "/watch a.testnet from -5", "/watch a.testnet since 5", "/watch a.testnet from 5 6", "/watch a.testnet from 99999999999999999999999"} {
		bot, w, fake := newBot()
		bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: text})
		if len(w.from) != 0 || len(w.watched) != 0 || len(fake.Sent) != 1 || !strings.HasPrefix(fake.Sent[0].Text, "Send /watch and one coin account") {
			t.Errorf("%q: from %v watched %v sent %+v", text, w.from, w.watched, fake.Sent)
		}
	}
}

func TestWatchFromRefusalsSayWhyAndAnotherChatGetsNothing(t *testing.T) {
	for err, wantText := range map[error]string{
		fmt.Errorf("%w: block 5, launch block 20", watchedcoins.ErrBeforeLaunch): "That block is before the launch block of a.testnet. Send a block from the launch on.",
		fmt.Errorf("%w: block 9, final head 7", watchedcoins.ErrAfterHead):       "That block is after the latest final block. Send a block that already exists.",
		errors.New("rpc down"): "I cannot read a.testnet. Check the account name and try again.",
	} {
		bot, w, fake := newBot()
		w.err = err
		bot.Handle(context.Background(), telegram.Incoming{ChatID: builderChat, Text: "/watch a.testnet from 5"})
		if len(fake.Sent) != 1 || fake.Sent[0].Text != wantText {
			t.Errorf("%v: sent %+v, want %q", err, fake.Sent, wantText)
		}
	}
	bot, w, fake := newBot()
	bot.Handle(context.Background(), telegram.Incoming{ChatID: 999, Text: "/watch a.testnet from 5"})
	if len(w.from) != 0 || len(fake.Sent) != 0 {
		t.Fatalf("another chat: from %v sent %+v", w.from, fake.Sent)
	}
}
