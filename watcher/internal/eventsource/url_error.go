package eventsource

import (
	"errors"
	"net/url"
)

// withoutURL drops the URL from a net/http error. The text of *url.Error holds the full URL, and a keyed
// provider URL holds its access token in the path. Only the cause stays.
func withoutURL(err error) error {
	var urlErr *url.Error
	if errors.As(err, &urlErr) {
		return urlErr.Err
	}
	return err
}
