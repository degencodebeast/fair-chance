package commands_test

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"

	"fairchance/watcher/internal/commands"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
)

const (
	strangerChat = 999
	factory      = "factory.testnet"
	demoCoin     = "bag.factory.testnet"
	pageLink     = "https://fair-chance.example/coin"
)

type clock struct{ now time.Time }

func (c *clock) Now() time.Time { return c.now }

func newPublicBot(checker *fixedChecker) (commands.Bot, *recordingWatcher, *telegram.Fake, *clock) {
	clk := &clock{now: time.Date(2026, 11, 14, 10, 0, 0, 0, time.UTC)}
	fake, w := &telegram.Fake{}, &recordingWatcher{}
	public, err := commands.NewPublicCheck(factory, pageLink, clk.Now)
	if err != nil {
		panic(err)
	}
	bot := commands.Bot{ChatID: builderChat, Reply: fake, Watcher: w, Checker: checker, Public: public}
	return bot, w, fake, clk
}

func say(bot commands.Bot, chat int64, text string) {
	bot.Handle(context.Background(), telegram.Incoming{ChatID: chat, Text: text})
}

func TestStrangerChatGetsTheCardOfAFactoryCoin(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, fake, _ := newPublicBot(checker)
	say(bot, strangerChat, "/check "+demoCoin)
	if len(checker.checked) != 1 || checker.checked[0] != demoCoin {
		t.Fatalf("checked %v", checker.checked)
	}
	if len(fake.Sent) != 1 || fake.Sent[0].ChatID != strangerChat || fake.Sent[0].Text != "card text" {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestStrangerWatchAndOtherTextGetNoAnswerAndChangeNothing(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, w, fake, _ := newPublicBot(checker)
	for _, text := range []string{"/watch " + demoCoin, "/stop", "hello", "/watch@bot " + demoCoin, "/check"} {
		say(bot, strangerChat, text)
	}
	if len(w.watched) != 0 || len(checker.checked) != 0 {
		t.Fatalf("watched %v checked %v", w.watched, checker.checked)
	}
	for _, m := range fake.Sent {
		if strings.Contains(m.Text, "Watching") {
			t.Fatalf("a stranger got a watch answer: %+v", m)
		}
	}
	if len(fake.Sent) > 1 {
		t.Fatalf("sent %+v", fake.Sent)
	}
}

func TestNothingIsEverSentToTheBuilderChatBecauseOfAStranger(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, fake, _ := newPublicBot(checker)
	say(bot, strangerChat, "/check "+demoCoin)
	say(bot, strangerChat, "/watch "+demoCoin)
	say(bot, strangerChat, "/start")
	for _, m := range fake.Sent {
		if m.ChatID == builderChat {
			t.Fatalf("sent to the builder chat: %+v", m)
		}
	}
}

func TestBuilderChatStillWatchesAndChecksAnyCoin(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, w, fake, _ := newPublicBot(checker)
	say(bot, builderChat, "/watch moth.other.testnet")
	say(bot, builderChat, "/check moth.other.testnet")
	if len(w.watched) != 1 || len(checker.checked) != 1 || len(fake.Sent) != 2 {
		t.Fatalf("watched %v checked %v sent %+v", w.watched, checker.checked, fake.Sent)
	}
}

func TestACoinOfAnotherFactoryMakesNoReadAndGetsOneLine(t *testing.T) {
	want := "I check only Fair Chance coins on testnet."
	for _, coin := range []string{"x.testnet", "bag.other.testnet", "a.bag.factory.testnet", factory, "badfactory.testnet", "bag.factory.near"} {
		checker := &fixedChecker{text: "card text"}
		bot, _, fake, _ := newPublicBot(checker)
		say(bot, strangerChat, "/check "+coin)
		if len(checker.checked) != 0 || len(fake.Sent) != 1 || fake.Sent[0].Text != want || fake.Sent[0].ChatID != strangerChat {
			t.Errorf("%s: checked %v sent %+v", coin, checker.checked, fake.Sent)
		}
	}
}

const waitLine = "Please wait a few seconds, then send it again."

func TestOneChatGetsOneCheckEveryThreeSecondsAndOneWaitLineBetween(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, fake, clk := newPublicBot(checker)
	say(bot, strangerChat, "/check "+demoCoin)
	clk.now = clk.now.Add(time.Second)
	say(bot, strangerChat, "/check "+demoCoin)
	clk.now = clk.now.Add(time.Second)
	say(bot, strangerChat, "/check "+demoCoin)
	if len(checker.checked) != 1 || len(fake.Sent) != 2 || fake.Sent[0].Text != "card text" || fake.Sent[1].Text != waitLine {
		t.Fatalf("checked %v sent %+v", checker.checked, fake.Sent)
	}
	clk.now = clk.now.Add(time.Second)
	say(bot, strangerChat, "/check "+demoCoin)
	if len(checker.checked) != 2 || len(fake.Sent) != 3 || fake.Sent[2].Text != "card text" {
		t.Fatalf("checked %v sent %+v", checker.checked, fake.Sent)
	}
	clk.now = clk.now.Add(time.Second)
	say(bot, strangerChat, "/check "+demoCoin)
	if len(fake.Sent) != 4 || fake.Sent[3].Text != waitLine {
		t.Fatalf("a new gap gets a new wait line: %+v", fake.Sent)
	}
}

func TestStartThenCheckAtOnceReturnsTheCard(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, fake, _ := newPublicBot(checker)
	say(bot, strangerChat, "/start")
	say(bot, strangerChat, "/check "+demoCoin)
	if len(checker.checked) != 1 || len(fake.Sent) != 2 || fake.Sent[1].Text != "card text" {
		t.Fatalf("checked %v sent %+v", checker.checked, fake.Sent)
	}
}

func TestOneStrangerChatCausesAtMostTwoSendsEveryThreeSeconds(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, fake, clk := newPublicBot(checker)
	for i := 0; i < 100; i++ {
		say(bot, strangerChat, "/check "+demoCoin)
		say(bot, strangerChat, "/check other.testnet")
		say(bot, strangerChat, "/help")
	}
	if len(fake.Sent) != 3 || len(checker.checked) != 1 {
		t.Fatalf("sends %d checks %d", len(fake.Sent), len(checker.checked))
	}
	clk.now = clk.now.Add(3 * time.Second)
	say(bot, strangerChat, "/help")
	if len(fake.Sent) != 4 {
		t.Fatalf("sends %d", len(fake.Sent))
	}
}

func TestStrangersTogetherCauseAtMostSixtySendsEveryMinute(t *testing.T) {
	bot, _, fake, clk := newPublicBot(&fixedChecker{text: "card text"})
	for i := 0; i < 200; i++ {
		say(bot, int64(2000+i), "/help")
	}
	if len(fake.Sent) != 60 {
		t.Fatalf("sends %d", len(fake.Sent))
	}
	clk.now = clk.now.Add(61 * time.Second)
	say(bot, 9000, "/help")
	if len(fake.Sent) != 61 {
		t.Fatalf("sends %d", len(fake.Sent))
	}
}

func TestTheBuilderChatIsNeverLimited(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, fake, _ := newPublicBot(checker)
	for i := 0; i < 100; i++ {
		say(bot, builderChat, "/check "+demoCoin)
	}
	if len(fake.Sent) != 100 || len(checker.checked) != 100 {
		t.Fatalf("sends %d checks %d", len(fake.Sent), len(checker.checked))
	}
}

func TestTheFactoryNameMustBeAValidLowercaseAccountWithADot(t *testing.T) {
	for _, bad := range []string{"testnet", ".factory.testnet", "Factory.testnet", "", "a..testnet", "factory.testnet."} {
		if _, err := commands.NewPublicCheck(bad, "", nil); err == nil {
			t.Errorf("%q was accepted", bad)
		}
	}
	if _, err := commands.NewPublicCheck("factory.testnet", "", nil); err != nil {
		t.Errorf("a valid name was refused: %v", err)
	}
}

func TestAllChatsTogetherGetThirtyChecksEveryMinute(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, fake, clk := newPublicBot(checker)
	for i := 0; i < 31; i++ {
		say(bot, int64(1000+i), "/check "+demoCoin)
	}
	if len(checker.checked) != 30 || fake.Sent[30].Text != "Busy. Try again in a minute." {
		t.Fatalf("checked %d, last reply %+v", len(checker.checked), fake.Sent[len(fake.Sent)-1])
	}
	clk.now = clk.now.Add(61 * time.Second)
	say(bot, 5000, "/check "+demoCoin)
	if len(checker.checked) != 31 {
		t.Fatalf("checked %d", len(checker.checked))
	}
}

func TestAnRPCRateLimitAnswersBusyAndMakesNoReadForAMinute(t *testing.T) {
	checker := &fixedChecker{err: fmt.Errorf("query: %w", eventsource.ErrRateLimited)}
	bot, _, fake, clk := newPublicBot(checker)
	say(bot, 1, "/check "+demoCoin)
	clk.now = clk.now.Add(20 * time.Second)
	say(bot, 2, "/check "+demoCoin)
	if len(checker.checked) != 1 {
		t.Fatalf("checked %v", checker.checked)
	}
	for _, m := range fake.Sent {
		if m.Text != "Busy. Try again in a minute." {
			t.Fatalf("sent %+v", fake.Sent)
		}
	}
	clk.now = clk.now.Add(41 * time.Second)
	checker.err, checker.text = nil, "card text"
	say(bot, 3, "/check "+demoCoin)
	if len(checker.checked) != 2 || fake.Sent[2].Text != "card text" {
		t.Fatalf("checked %v sent %+v", checker.checked, fake.Sent)
	}
}

func TestAReadFailureOfAStrangerSaysSoWithoutBackingOff(t *testing.T) {
	checker := &fixedChecker{err: errors.New("rpc down")}
	bot, _, fake, clk := newPublicBot(checker)
	say(bot, 1, "/check "+demoCoin)
	clk.now = clk.now.Add(time.Second)
	say(bot, 2, "/check "+demoCoin)
	if len(checker.checked) != 2 || !strings.HasPrefix(fake.Sent[0].Text, "I cannot read "+demoCoin) {
		t.Fatalf("checked %v sent %+v", checker.checked, fake.Sent)
	}
}

func TestStartAndHelpFromAnyChatAnswerOneLineWithThePageLink(t *testing.T) {
	for _, text := range []string{"/start", "/help", "/start@fair_chance_bot"} {
		for _, chat := range []int64{strangerChat, builderChat} {
			bot, _, fake, _ := newPublicBot(&fixedChecker{})
			say(bot, chat, text)
			if len(fake.Sent) != 1 || fake.Sent[0].ChatID != chat || strings.Contains(fake.Sent[0].Text, "\n") ||
				!strings.Contains(fake.Sent[0].Text, "/check") || !strings.Contains(fake.Sent[0].Text, pageLink) {
				t.Errorf("%s from %d: sent %+v", text, chat, fake.Sent)
			}
		}
	}
}

func TestCountsHoldNumbersAndNoChatIDOrText(t *testing.T) {
	checker := &fixedChecker{text: "card text"}
	bot, _, _, _ := newPublicBot(checker)
	say(bot, strangerChat, "/check "+demoCoin)
	say(bot, strangerChat, "/check "+demoCoin)
	say(bot, strangerChat, "/check "+demoCoin)
	say(bot, 5, "/check other.testnet")
	got := bot.Public.Counts()
	if got.Served != 1 || got.Silent != 1 || got.Refused != 1 {
		t.Fatalf("counts %+v", got)
	}
}
