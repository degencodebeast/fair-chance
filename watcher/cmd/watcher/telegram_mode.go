package main

import (
	"context"
	"fmt"
	"log"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"time"

	"fairchance/watcher/internal/checkcard"
	"fairchance/watcher/internal/commands"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
	"fairchance/watcher/internal/urlsafe"
	"fairchance/watcher/internal/watchedcoins"
)

const defaultFastNearTestnet = "https://tx.test.fastnear.com"

// runTelegram runs the bot: long polling for /watch and /check, and a read pass for alerts.
// Environment: FC_RPC_URL, FC_TELEGRAM_CHAT_ID (the one chat the bot answers), TELEGRAM_BOT_TOKEN
// (read here and nowhere else), FC_EVENT_SOURCE ("block-scan" by default, or "fastnear" with
// FC_FASTNEAR_URL), FC_COINS (optional coins to watch from the start). FC_PUBLIC_FACTORY (optional, the
// testnet factory account) lets any chat send /check for a coin of that factory, rate limited; FC_PAGE_URL
// (optional) is the hosted page link in the /start and /help line. Alerts and /watch stay in the one chat.
func runTelegram(poll time.Duration) error {
	rpcURL := os.Getenv("FC_RPC_URL")
	chatID, err := strconv.ParseInt(os.Getenv("FC_TELEGRAM_CHAT_ID"), 10, 64)
	if rpcURL == "" || err != nil || chatID == 0 {
		return fmt.Errorf("set FC_RPC_URL and FC_TELEGRAM_CHAT_ID")
	}
	if err := urlsafe.CheckEndpoint("FC_RPC_URL", rpcURL); err != nil {
		return err
	}
	var src eventsource.Source
	switch kind := os.Getenv("FC_EVENT_SOURCE"); kind {
	case "", "block-scan":
		src = eventsource.NewRPCScan(rpcURL)
	case "fastnear":
		url := os.Getenv("FC_FASTNEAR_URL")
		if url == "" {
			url = defaultFastNearTestnet
		}
		if err := urlsafe.CheckEndpoint("FC_FASTNEAR_URL", url); err != nil {
			return err
		}
		src = eventsource.NewFastNear(url)
	default:
		return fmt.Errorf("FC_EVENT_SOURCE=%q: use block-scan or fastnear", kind)
	}
	// The endpoints pass their checks before the bot token is read.
	api, err := telegram.NewBotAPIFromEnvironment()
	if err != nil {
		return err
	}

	set := &watchedcoins.Set{
		Source: src,
		Head:   func(ctx context.Context) (eventsource.Cursor, error) { return eventsource.HeadCursor(ctx, rpcURL) },
		Facts: func(ctx context.Context, coin string) (eventsource.CoinFacts, error) {
			return eventsource.ReadCoinFacts(ctx, rpcURL, coin)
		},
		Sender:   api,
		ChatID:   chatID,
		Location: time.Local,
	}
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt)
	defer stop()
	for _, coin := range strings.Split(os.Getenv("FC_COINS"), ",") {
		if coin = strings.TrimSpace(coin); coin != "" {
			if err := set.Watch(ctx, coin); err != nil {
				return err
			}
		}
	}

	go func() {
		for {
			if err := set.Poll(ctx); err != nil && ctx.Err() == nil {
				if stop := set.Stopped(); stop != nil {
					log.Printf("reads stopped for good, no retry, restart the watcher after you check the source: %v", stop)
					return
				}
				log.Printf("read pass: %v", err)
			}
			select {
			case <-ctx.Done():
				return
			case <-time.After(poll):
			}
		}
	}()
	bot := commands.Bot{ChatID: chatID, Reply: api, Watcher: set,
		Checker: checkcard.Checker{RPCURL: rpcURL, Location: time.Local}}
	if factory := strings.TrimSpace(os.Getenv("FC_PUBLIC_FACTORY")); factory != "" {
		public, err := commands.NewPublicCheck(factory, strings.TrimSpace(os.Getenv("FC_PAGE_URL")), nil)
		if err != nil {
			return fmt.Errorf("FC_PUBLIC_FACTORY: %w", err)
		}
		bot.Public = public
		go logPublicCounts(ctx, bot.Public)
	}
	if err := bot.Serve(ctx, api); err != nil && ctx.Err() == nil {
		return err
	}
	return nil
}

// logPublicCounts logs numbers only, never message text or chat IDs, and only when they changed.
func logPublicCounts(ctx context.Context, p *commands.PublicCheck) {
	var last commands.PublicCounts
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(10 * time.Minute):
		}
		if now := p.Counts(); now != last {
			log.Printf("public check: served %d, busy %d, refused %d", now.Served, now.Busy, now.Refused)
			last = now
		}
	}
}
