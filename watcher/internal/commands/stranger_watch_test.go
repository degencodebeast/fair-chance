package commands_test

import (
	"testing"
	"time"
)

const strangerWatchText = "Alerts are for the coin's team for now. Use /check to read a coin's rules."

func TestStrangerWatchGetsOneExactReplyAndStartsNothing(t *testing.T) {
	for _, text := range []string{"/watch " + demoCoin, "/watch@fair_chance_bot " + demoCoin, "/watch"} {
		bot, w, fake, clk := newPublicBot(&fixedChecker{})
		say(bot, strangerChat, text)
		if len(fake.Sent) != 1 || fake.Sent[0].ChatID != strangerChat || fake.Sent[0].Text != strangerWatchText || len(w.watched) != 0 {
			t.Fatalf("%q: sent %+v watched %v", text, fake.Sent, w.watched)
		}
		say(bot, strangerChat, text) // inside the per chat gap: silence
		if len(fake.Sent) != 1 {
			t.Fatalf("%q: a second reply inside the gap", text)
		}
		clk.now = clk.now.Add(time.Hour)
		say(bot, strangerChat, text)
		if len(fake.Sent) != 2 {
			t.Fatalf("%q: no reply after the gap", text)
		}
	}
}

// /watch shares the per chat gap of /start and /help, so a stranger cannot get more replies by mixing them.
func TestStrangerWatchSharesTheHelpGap(t *testing.T) {
	bot, _, fake, _ := newPublicBot(&fixedChecker{})
	say(bot, strangerChat, "/start")
	say(bot, strangerChat, "/watch "+demoCoin)
	if len(fake.Sent) != 1 {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestConfiguredChatWatchStillWorksWithPublicSet(t *testing.T) {
	bot, w, fake, _ := newPublicBot(&fixedChecker{})
	say(bot, builderChat, "/watch "+demoCoin)
	if len(w.watched) != 1 || len(fake.Sent) != 1 || fake.Sent[0].Text == strangerWatchText {
		t.Fatalf("sent %+v watched %v", fake.Sent, w.watched)
	}
}
