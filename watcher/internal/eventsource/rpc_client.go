package eventsource

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"
)

type rpcClient struct {
	url  string
	http *http.Client
}

func newRPC(url string) *rpcClient {
	return &rpcClient{url: url, http: &http.Client{Timeout: 60 * time.Second}}
}

type rpcError struct {
	Name  string `json:"name"`
	Cause struct {
		Name string `json:"name"`
	} `json:"cause"`
	Data    any    `json:"data"`
	Message string `json:"message"`
}

func (e *rpcError) Error() string {
	return fmt.Sprintf("%s/%s: %v", e.Name, e.Cause.Name, e.Data)
}

// errNoBlockAtHeight means the chain has no block at this height. It is not lost data.
var errNoBlockAtHeight = errors.New("no block at this height")

func (c *rpcClient) call(ctx context.Context, method string, params any, out any) error {
	body, err := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": "fc", "method": method, "params": params})
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, c.url, bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.http.Do(req)
	if err != nil {
		return withoutURL(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusTooManyRequests {
		return fmt.Errorf("%s: HTTP 429: %w", method, ErrRateLimited)
	}
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("%s: HTTP %d", method, resp.StatusCode)
	}
	var envelope struct {
		Result json.RawMessage `json:"result"`
		Error  *rpcError       `json:"error"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&envelope); err != nil {
		return fmt.Errorf("%s: %w", method, err)
	}
	if envelope.Error != nil {
		// A node answers UNKNOWN_BLOCK with "BLOCK HEIGHT" for a height that has no block, and
		// with a block hash when the block existed but its data is gone (garbage collected).
		if envelope.Error.Cause.Name == "UNKNOWN_BLOCK" && strings.Contains(fmt.Sprint(envelope.Error.Data), "BLOCK HEIGHT") {
			return errNoBlockAtHeight
		}
		return fmt.Errorf("%s: %w", method, envelope.Error)
	}
	return json.Unmarshal(envelope.Result, out)
}

func (c *rpcClient) view(ctx context.Context, account, method string, out any) error {
	var res struct {
		Result []byte `json:"result"`
	}
	params := map[string]any{
		"request_type": "call_function", "finality": "final", "account_id": account,
		"method_name": method, "args_base64": base64.StdEncoding.EncodeToString([]byte("{}")),
	}
	if err := c.call(ctx, "query", params, &res); err != nil {
		return err
	}
	return json.Unmarshal(res.Result, out)
}
