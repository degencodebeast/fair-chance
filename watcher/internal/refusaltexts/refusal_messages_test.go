package refusaltexts_test

import (
	"encoding/json"
	"os"
	"testing"

	"fairchance/watcher/internal/refusaltexts"
)

// Written by the rule engine test: every refusal message the contract can return.
const everyMessagePath = "../../../refusal-texts/every-refusal-message.json"

type contractRefusal struct {
	Refusal string `json:"refusal"`
	Message string `json:"message"`
}

func loadTexts(t *testing.T) refusaltexts.Texts {
	t.Helper()
	texts, err := refusaltexts.Load(textsPath)
	if err != nil {
		t.Fatal(err)
	}
	return texts
}

func TestEveryRefusalTheContractCanReturnResolvesToAPlainSentence(t *testing.T) {
	texts := loadTexts(t)
	raw, err := os.ReadFile(everyMessagePath)
	if err != nil {
		t.Fatal(err)
	}
	var refusals []contractRefusal
	if err := json.Unmarshal(raw, &refusals); err != nil {
		t.Fatal(err)
	}
	if len(refusals) < 15 {
		t.Fatalf("want at least 15 contract refusals, got %d", len(refusals))
	}
	for _, r := range refusals {
		// The chain wraps a panic message; the reader finds it inside the wrapper too.
		for _, message := range []string{r.Message, "Smart contract panicked: " + r.Message} {
			sentence, ok := texts.ForMessage(message)
			if !ok || sentence == "" {
				t.Errorf("%s: no plain sentence for %q", r.Refusal, message)
			}
		}
	}
}

func TestANotRegisteredRefusalResolvesForAnyAccountInBothForms(t *testing.T) {
	texts := loadTexts(t)
	want := texts["The account is not registered: <account>"]
	if want == "" {
		t.Fatal("no sentence for the not registered refusal")
	}
	for _, message := range []string{
		"Smart contract panicked: The account is not registered: alice.testnet",
		"The account is not registered: 0123abcdef0123abcdef0123abcdef0123abcdef0123abcdef0123abcdef0123",
	} {
		if got, _ := texts.ForMessage(message); got != want {
			t.Errorf("%q: got %q, want %q", message, got, want)
		}
	}
	withdraw := texts["The account <account> is not registered"]
	if got, _ := texts.ForMessage("Smart contract panicked: The account bob.near is not registered"); got != withdraw || withdraw == "" {
		t.Errorf("storage_withdraw form: got %q, want %q", got, withdraw)
	}
	if got, _ := texts.ForMessage("The account doesn't have enough balance"); got == want || got == withdraw {
		t.Errorf("the balance refusal resolved to the not registered sentence %q", got)
	}
}

func TestAMessageWithNoKnownRefusalHasNoSentence(t *testing.T) {
	texts := loadTexts(t)
	for _, message := range []string{"Exceeded the prepaid gas", "FC_NO_SUCH_CODE", "The account bob.near"} {
		if got, ok := texts.ForMessage(message); ok {
			t.Errorf("%q: want no sentence, got %q", message, got)
		}
	}
}
