package main

import (
	"flag"
	"testing"
	"time"
)

func TestThePollDefaultIsTwoSeconds(t *testing.T) {
	fs := flag.NewFlagSet("t", flag.ContinueOnError)
	poll := pollFlag(fs)
	if err := fs.Parse(nil); err != nil || *poll != 2*time.Second {
		t.Fatalf("poll %v, err %v", *poll, err)
	}
}
