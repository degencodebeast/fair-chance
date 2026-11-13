// Package refusaltexts loads the shared file that maps each FC_ refusal code to one plain sentence.
// The bot and the web screens read the same file, so no refusal text is written twice.
package refusaltexts

import (
	"encoding/json"
	"fmt"
	"os"
)

// Load reads the file at path: a JSON object from refusal code to sentence.
func Load(path string) (map[string]string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var texts map[string]string
	if err := json.Unmarshal(raw, &texts); err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	return texts, nil
}
