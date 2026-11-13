package checkcard_test

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// The alerts screen shows the /check reply from the golden file of the cap-full fixture.
func TestTheAlertsScreenSamplesTheCheckReplyOfTheCapFullFixture(t *testing.T) {
	raw, err := os.ReadFile("../../../testdata/watcher-sample-messages/messages.json")
	if err != nil {
		t.Fatal(err)
	}
	var sample struct {
		CheckReply string `json:"check_reply"`
	}
	if err := json.Unmarshal(raw, &sample); err != nil {
		t.Fatal(err)
	}
	want, err := os.ReadFile("testdata/normal_cap_full.golden.txt")
	if err != nil {
		t.Fatal(err)
	}
	if sample.CheckReply != strings.TrimSuffix(string(want), "\n") {
		t.Fatalf("the sample says:\n%s\nthe golden file says:\n%s", sample.CheckReply, want)
	}
}
