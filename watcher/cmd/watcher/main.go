// Command watcher reads coin events and sends one alert per limited-sell event.
//
// Environment: FC_RPC_URL (NEAR RPC endpoint), FC_COINS (comma-separated coin accounts),
// FC_SENDER ("fake": alerts go to an in-memory sender and are printed as
// JSON, with FC_CHAT_ID optional; or "telegram": see runTelegram). It holds no NEAR keys and signs nothing.
//
// With -once it stops after -want alerts or after -timeout, prints {"alerts": [...]}, and
// exits 0 only when it got the alerts it wanted.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"log"
	"os"
	"strconv"
	"strings"
	"time"

	"fairchance/watcher/internal/alerts"
	"fairchance/watcher/internal/eventsource"
	"fairchance/watcher/internal/telegram"
	"fairchance/watcher/internal/urlsafe"
)

func main() {
	once := flag.Bool("once", false, "stop after -want alerts or -timeout")
	want := flag.Int("want", 1, "alerts to wait for with -once")
	timeout := flag.Duration("timeout", 60*time.Second, "time limit with -once")
	poll := flag.Duration("poll", time.Second, "time between read passes")
	flag.Parse()
	log.SetFlags(0)
	log.SetPrefix("watcher: ")
	// No URL reaches the log: an RPC URL could hold a key, and a Telegram request URL holds the token.
	log.SetOutput(urlsafe.Writer{W: os.Stderr})

	if os.Getenv("FC_SENDER") == "telegram" {
		if err := runTelegram(*poll); err != nil {
			log.Fatal(err)
		}
		return
	}

	rpcURL := os.Getenv("FC_RPC_URL")
	coins := strings.Split(os.Getenv("FC_COINS"), ",")
	if rpcURL == "" || os.Getenv("FC_COINS") == "" {
		log.Fatal("set FC_RPC_URL and FC_COINS")
	}
	if err := urlsafe.CheckEndpoint("FC_RPC_URL", rpcURL); err != nil {
		log.Fatal(err)
	}
	if s := os.Getenv("FC_SENDER"); s != "fake" {
		log.Fatalf("FC_SENDER=%q: use fake or telegram", s)
	}
	chatID, _ := strconv.ParseInt(os.Getenv("FC_CHAT_ID"), 10, 64)

	ctx := context.Background()
	if *once {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, *timeout)
		defer cancel()
	}
	if err := run(ctx, rpcURL, coins, chatID, *once, *want, *poll); err != nil {
		log.Fatal(err)
	}
}

func run(ctx context.Context, rpcURL string, coins []string, chatID int64, once bool, want int, poll time.Duration) error {
	cursor := eventsource.Cursor(^uint64(0))
	for _, coin := range coins {
		c, err := eventsource.LaunchCursor(ctx, rpcURL, coin)
		if err != nil {
			return err
		}
		cursor = min(cursor, c)
	}
	src := eventsource.NewRPCScan(rpcURL)
	fake := &telegram.Fake{}
	printed := 0
	for {
		_, next, err := alerts.SendNew(ctx, src, coins, cursor, alerts.Writer{Location: time.Local}, fake, chatID)
		cursor = next
		if err != nil && ctx.Err() == nil {
			if errors.Is(err, eventsource.ErrForbidden) || errors.Is(err, eventsource.ErrUnexpectedResponse) ||
				errors.Is(err, eventsource.ErrRPCForbidden) || errors.Is(err, eventsource.ErrRPCWrongSetup) {
				return fmt.Errorf("reads stopped for good, no retry: %w", err)
			}
			log.Printf("read pass: %v", err)
		}
		if !once {
			for ; printed < len(fake.Sent); printed++ {
				line, _ := json.Marshal(fake.Sent[printed])
				fmt.Println(string(line))
			}
		}
		if once && (len(fake.Sent) >= want || ctx.Err() != nil) {
			out, _ := json.Marshal(map[string]any{"alerts": fake.Sent})
			fmt.Println(string(out))
			if len(fake.Sent) < want {
				return fmt.Errorf("got %d alerts, wanted %d", len(fake.Sent), want)
			}
			return nil
		}
		select {
		case <-ctx.Done():
		case <-time.After(poll):
		}
	}
}
