package eventsource

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/url"
)

// transientStatus is an HTTP status of a short fault: a server fault, or a timeout (408, which is nearcore's
// TIMEOUT_ERROR) or a too early answer (425).
func transientStatus(code int) bool {
	return code >= 500 || code == http.StatusRequestTimeout || code == http.StatusTooEarly
}

// transientBodyError reports whether a failed body read is a short fault: a cut or slow body, a reset,
// a timeout. A body that is wrong JSON (a syntax or type error) is not: it means a wrong setup.
func transientBodyError(err error) bool {
	var syntax *json.SyntaxError
	var typed *json.UnmarshalTypeError
	return !errors.As(err, &syntax) && !errors.As(err, &typed)
}

// withoutURL drops the URL from a net/http error. The text of *url.Error holds the full URL, and a keyed
// provider URL holds its access token in the path. Only the cause stays.
func withoutURL(err error) error {
	var urlErr *url.Error
	if errors.As(err, &urlErr) {
		return urlErr.Err
	}
	return err
}
