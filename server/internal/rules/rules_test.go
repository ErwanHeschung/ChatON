package rules

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"

	"fiatjaf.com/nostr"
)

func TestParse(t *testing.T) {
	r, err := Parse(nil, nil)
	if err != nil || r.Whitelist || !r.AllowsInstance("any.example") || !r.AllowsInstance("") || r.Limits != DefaultLimits {
		t.Fatalf("defaults: %+v %v", r, err)
	}
	pk := nostr.Generate().Public()
	r, err = Parse([]byte("mode: blacklist\ninstances: [Spam.Example]\nlimits: {events_per_minute: 5}\n"), []string{pk.Hex()})
	if err != nil || r.AllowsInstance("spam.example") || !r.AllowsInstance("ok.example") || !r.Blocked(pk) {
		t.Fatalf("blacklist: %+v %v", r, err)
	}
	if r.Limits.EventsPerMinute != 5 || r.Limits.EventsBurst != DefaultLimits.EventsBurst {
		t.Errorf("limits merge with defaults: %+v", r.Limits)
	}
	r, err = Parse([]byte("mode: whitelist\ninstances: [friend.example]\n"), nil)
	if err != nil || !r.AllowsInstance("friend.example") || r.AllowsInstance("other.example") || r.AllowsInstance("") {
		t.Fatalf("whitelist: %+v %v", r, err)
	}
	if _, err := Parse([]byte("mode: greylist\n"), nil); err == nil {
		t.Error("unknown mode accepted")
	}
	if _, err := Parse([]byte("keys: [nothex]\n"), nil); err == nil {
		t.Error("bad key accepted")
	}
}

func TestStoreAndWellKnown(t *testing.T) {
	s, err := Open(filepath.Join(t.TempDir(), "missing.yaml"), nil)
	if err != nil || s.Get().Whitelist {
		t.Fatalf("missing file: %v", err)
	}
	s = Fixed(&Rules{Whitelist: true, Instances: []string{"friend.example"}, Keys: []nostr.PubKey{nostr.Generate().Public()}})
	mux := http.NewServeMux()
	s.Register(mux)
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest("GET", "/.well-known/chaton/rules", nil))
	var got map[string]any
	if err := json.Unmarshal(rec.Body.Bytes(), &got); err != nil || got["mode"] != "whitelist" || len(got) != 2 {
		t.Fatalf("well-known: %s", rec.Body)
	}
}

func TestVerifier(t *testing.T) {
	pk := nostr.Generate().Public()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"names": map[string]string{r.URL.Query().Get("name"): pk.Hex()}})
	}))
	defer srv.Close()
	host := strings.TrimPrefix(srv.URL, "http://127.0.0.1") // the dev rule: http for localhost
	v := Verifier{Client: srv.Client()}
	if d, err := v.Instance(context.Background(), "bob@localhost"+host, pk); err != nil || d != "localhost"+host {
		t.Fatalf("instance: %q %v", d, err)
	}
	if _, err := v.Instance(context.Background(), "bob@localhost"+host, nostr.Generate().Public()); err == nil {
		t.Error("other key accepted")
	}
	if _, err := v.Instance(context.Background(), "bob@evil/x", pk); err == nil {
		t.Error("path in domain accepted")
	}
}
