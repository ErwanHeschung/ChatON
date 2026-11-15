// Package nip98 checks HTTP requests signed with a Nostr event (NIP-98).
package nip98

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"net/http"
	"strings"
	"time"

	"fiatjaf.com/nostr"
)

const maxSkew = 60 * time.Second

// Verify checks an `Authorization: Nostr <base64 event>` header (NIP-98)
// for a request to url with the given body, and returns the signer's pubkey.
func Verify(r *http.Request, url string, body []byte, now time.Time) (nostr.PubKey, error) {
	b64, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Nostr ")
	if !ok {
		return nostr.ZeroPK, errors.New("missing Nostr authorization")
	}
	raw, err := base64.StdEncoding.DecodeString(b64)
	if err != nil {
		return nostr.ZeroPK, errors.New("authorization is not base64")
	}
	var ev nostr.Event
	if err := json.Unmarshal(raw, &ev); err != nil {
		return nostr.ZeroPK, errors.New("authorization is not an event")
	}
	if ev.Kind != nostr.KindHTTPAuth {
		return nostr.ZeroPK, errors.New("wrong kind")
	}
	if !ev.CheckID() || !ev.VerifySignature() {
		return nostr.ZeroPK, errors.New("bad signature")
	}
	if d := now.Sub(ev.CreatedAt.Time()); d > maxSkew || d < -maxSkew {
		return nostr.ZeroPK, errors.New("expired")
	}
	sum := sha256.Sum256(body)
	if tagValue(ev, "u") != url || tagValue(ev, "method") != r.Method || tagValue(ev, "payload") != hex.EncodeToString(sum[:]) {
		return nostr.ZeroPK, errors.New("u, method or payload mismatch")
	}
	return ev.PubKey, nil
}

func tagValue(ev nostr.Event, name string) string {
	if t := ev.Tags.Find(name); len(t) > 1 {
		return t[1]
	}
	return ""
}
