// Package refusaltexts loads the shared file that maps each contract refusal to one plain sentence.
// The bot and the web screens read the same file, so no refusal text is written twice.
package refusaltexts

import (
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"strings"
)

// Texts maps a refusal key to its sentence. A key is an FC_ code, or the fixed text of a
// standard NEP-141 or NEP-145 message, with AccountPlaceholder where the message names an account.
type Texts map[string]string

// AccountPlaceholder marks where a standard message names an account, which changes per call.
const AccountPlaceholder = "<account>"

var fcCode = regexp.MustCompile(`FC_[A-Z_]+`)

// Load reads the file at path: a JSON object from refusal key to sentence.
func Load(path string) (Texts, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var texts Texts
	if err := json.Unmarshal(raw, &texts); err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	return texts, nil
}

// ForMessage gives the sentence for a contract failure message, raw or wrapped by the chain
// ("Smart contract panicked: ..."). An FC_ code in the message picks its sentence. Otherwise a
// standard message matches by its fixed text, with any account in place of the placeholder.
func (t Texts) ForMessage(message string) (string, bool) {
	if code := fcCode.FindString(message); code != "" {
		sentence, ok := t[code]
		return sentence, ok
	}
	for key, sentence := range t {
		if !strings.HasPrefix(key, "FC_") && matchesStandardMessage(key, message) {
			return sentence, true
		}
	}
	return "", false
}

// matchesStandardMessage reports whether message holds key, where the key's placeholder stands
// for one account ID (no spaces).
func matchesStandardMessage(key, message string) bool {
	before, after, hasAccount := strings.Cut(key, AccountPlaceholder)
	if !hasAccount {
		return strings.Contains(message, key)
	}
	start := strings.Index(message, before)
	if start < 0 {
		return false
	}
	rest := message[start+len(before):]
	account := rest
	if end := strings.IndexAny(rest, " \t\n\"'"); end >= 0 {
		account = rest[:end]
	}
	return account != "" && strings.HasPrefix(rest[len(account):], after)
}
