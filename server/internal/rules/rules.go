// Package rules: the instance's rules file, rules.yaml (spec/federation.md § 6).
// Blocked keys, blocked (blacklist) or allowed (whitelist) instances, rate
// limits. Keys and instances reload on SIGHUP; limits apply at startup.
package rules

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"net/http"
	"os"
	"slices"
	"strings"
	"sync/atomic"

	"fiatjaf.com/nostr"
	"go.yaml.in/yaml/v3"
)

type Limits struct {
	EventsPerMinute            int `yaml:"events_per_minute"`
	EventsBurst                int `yaml:"events_burst"`
	ConnectionsPerMinute       int `yaml:"connections_per_minute"`
	ConnectionsBurst           int `yaml:"connections_burst"`
	WrapsPerRecipientPerMinute int `yaml:"wraps_per_recipient_per_minute"`
}

// DefaultLimits: what the relay had before rules.yaml.
var DefaultLimits = Limits{
	EventsPerMinute:            20,
	EventsBurst:                60,
	ConnectionsPerMinute:       10,
	ConnectionsBurst:           30,
	WrapsPerRecipientPerMinute: 60,
}

type file struct {
	Mode      string   `yaml:"mode"`
	Instances []string `yaml:"instances"`
	Keys      []string `yaml:"keys"`
	Limits    Limits   `yaml:"limits"`
}

type Rules struct {
	// Whitelist: only the listed instances; else (default) all but them.
	Whitelist bool
	Instances []string
	Keys      []nostr.PubKey
	Limits    Limits
}

// Parse reads rules.yaml; empty means the defaults. extraKeys: SERVER_BLOCKEDPUBKEYS.
func Parse(data []byte, extraKeys []string) (*Rules, error) {
	f := file{Limits: DefaultLimits}
	if err := yaml.Unmarshal(data, &f); err != nil {
		return nil, fmt.Errorf("rules: %w", err)
	}
	r := &Rules{Limits: f.Limits}
	switch f.Mode {
	case "", "blacklist":
	case "whitelist":
		r.Whitelist = true
	default:
		return nil, fmt.Errorf("rules: mode %q: blacklist or whitelist", f.Mode)
	}
	for _, domain := range f.Instances {
		r.Instances = append(r.Instances, Domain(domain))
	}
	for _, hex := range append(f.Keys, extraKeys...) {
		pk, err := nostr.PubKeyFromHex(strings.TrimSpace(hex))
		if err != nil {
			return nil, fmt.Errorf("rules: key %q: %w", hex, err)
		}
		r.Keys = append(r.Keys, pk)
	}
	return r, nil
}

// Domain: how instances compare, lowercase host[:port].
func Domain(s string) string {
	return strings.ToLower(strings.TrimSpace(s))
}

func (r *Rules) Blocked(pk nostr.PubKey) bool {
	return slices.Contains(r.Keys, pk)
}

// AllowsInstance: may someone from this instance take part? "" is unknown:
// allowed in blacklist mode, not in whitelist mode.
func (r *Rules) AllowsInstance(domain string) bool {
	if domain == "" {
		return !r.Whitelist
	}
	return slices.Contains(r.Instances, Domain(domain)) == r.Whitelist
}

// Store holds the current rules; Reload swaps them (SIGHUP).
type Store struct {
	cur   atomic.Pointer[Rules]
	path  string
	extra []string
}

// Open loads path; a missing file means the defaults (blacklist, nothing blocked).
func Open(path string, extraKeys []string) (*Store, error) {
	s := &Store{path: path, extra: extraKeys}
	return s, s.Reload()
}

// Fixed rules, for tests.
func Fixed(r *Rules) *Store {
	s := &Store{}
	s.cur.Store(r)
	return s
}

func (s *Store) Get() *Rules { return s.cur.Load() }

func (s *Store) Reload() error {
	data, err := os.ReadFile(s.path)
	if err != nil && !errors.Is(err, fs.ErrNotExist) {
		return fmt.Errorf("rules: %w", err)
	}
	r, err := Parse(data, s.extra)
	if err != nil {
		return err
	}
	s.cur.Store(r)
	return nil
}

// Register serves GET /.well-known/chaton/rules: mode and instances, for the
// clients of this instance, which apply them to DMs. Keys and limits stay private.
func (s *Store) Register(mux *http.ServeMux) {
	mux.HandleFunc("GET /.well-known/chaton/rules", func(w http.ResponseWriter, _ *http.Request) {
		r := s.Get()
		mode := "blacklist"
		if r.Whitelist {
			mode = "whitelist"
		}
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"mode": mode, "instances": append([]string{}, r.Instances...)})
	})
}
