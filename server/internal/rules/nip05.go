package rules

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strings"

	"fiatjaf.com/nostr"
)

// Verifier checks a handle against its instance (NIP-05), to know where a key
// comes from. Client: safehttp, so another instance can't make us reach inside.
type Verifier struct {
	Client *http.Client
}

// Instance returns the domain of handle if handle (`name@domain`) names pk there.
func (v Verifier) Instance(ctx context.Context, handle string, pk nostr.PubKey) (string, error) {
	name, domain, ok := strings.Cut(strings.ToLower(strings.TrimPrefix(handle, "@")), "@")
	if !ok || name == "" || domain == "" || strings.ContainsAny(domain, "/?#@") {
		return "", fmt.Errorf("invalid handle %q", handle)
	}
	scheme := "https"
	if host, _, _ := strings.Cut(domain, ":"); host == "localhost" || host == "127.0.0.1" {
		scheme = "http" // dev, as in core
	}
	u := scheme + "://" + domain + "/.well-known/nostr.json?name=" + url.QueryEscape(name)
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u, nil)
	if err != nil {
		return "", fmt.Errorf("nip05: %w", err)
	}
	resp, err := v.Client.Do(req)
	if err != nil {
		return "", fmt.Errorf("nip05: %w", err)
	}
	defer resp.Body.Close()
	var doc struct {
		Names map[string]string `json:"names"`
	}
	if err := json.NewDecoder(io.LimitReader(resp.Body, 64<<10)).Decode(&doc); err != nil {
		return "", fmt.Errorf("nip05: %w", err)
	}
	if doc.Names[name] != pk.Hex() {
		return "", fmt.Errorf("nip05: %s is not this key", handle)
	}
	return Domain(domain), nil
}
