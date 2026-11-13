package refusaltexts_test

import (
	"os"
	"regexp"
	"strings"
	"testing"

	"fairchance/watcher/internal/refusaltexts"
)

const (
	textsPath     = "../../../refusal-texts/refusal-texts.json"
	interfacePath = "../../../docs/INTERFACE.md"
)

// Codes that INTERFACE.md names only to say they were renamed.
var retiredCodes = map[string]bool{"FC_TRANSFER_CALL_NOT_POOL": true}

func TestEveryRefusalCodeInTheInterfaceHasOneSentence(t *testing.T) {
	texts, err := refusaltexts.Load(textsPath)
	if err != nil {
		t.Fatal(err)
	}
	doc, err := os.ReadFile(interfacePath)
	if err != nil {
		t.Fatal(err)
	}
	codes := regexp.MustCompile(`FC_[A-Z_]+`).FindAllString(string(doc), -1)
	if len(codes) == 0 {
		t.Fatal("no FC_ code found in the interface file")
	}
	for _, code := range codes {
		if retiredCodes[code] {
			continue
		}
		if strings.TrimSpace(texts[code]) == "" {
			t.Errorf("no sentence for %s", code)
		}
	}
	for code := range texts {
		if !strings.HasPrefix(code, "FC_") {
			continue // a standard NEP message, not an interface code
		}
		if retiredCodes[code] || !strings.Contains(string(doc), code) {
			t.Errorf("%s is in the text file but is not a current code in the interface file", code)
		}
	}
}

func TestRefusalSentencesFollowTheCopyRules(t *testing.T) {
	texts, err := refusaltexts.Load(textsPath)
	if err != nil {
		t.Fatal(err)
	}
	banned := regexp.MustCompile(`(?i)\b(safe|rug-proof|audited|guaranteed|first|only|queued|pending|waits)\b|—`)
	for code, text := range texts {
		if m := banned.FindString(text); m != "" {
			t.Errorf("%s: sentence %q has %q", code, text, m)
		}
		if strings.Count(text, ". ") > 0 || !strings.HasSuffix(text, ".") {
			t.Errorf("%s: want one sentence ending in a full stop, got %q", code, text)
		}
	}
}
