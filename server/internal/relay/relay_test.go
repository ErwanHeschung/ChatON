package relay

import (
	"context"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/fiatjaf/eventstore/sqlite3"
	"github.com/nbd-wtf/go-nostr"
)

type fakeDir map[string]bool

func (d fakeDir) IsLocal(pk string) bool { return d[pk] }

type user struct{ sk, pk string }

func newUser() user {
	sk := nostr.GeneratePrivateKey()
	pk, _ := nostr.GetPublicKey(sk)
	return user{sk, pk}
}

// startRelay runs a relay where the given users hold a handle.
func startRelay(t *testing.T, locals ...user) string {
	t.Helper()
	db := &sqlite3.SQLite3Backend{DatabaseURL: filepath.Join(t.TempDir(), "r.db")}
	if err := db.Init(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(db.Close)
	dir := fakeDir{}
	for _, u := range locals {
		dir[u.pk] = true
	}
	srv := httptest.NewServer(New(Config{Name: "test"}, db, dir))
	t.Cleanup(srv.Close)
	return "ws" + strings.TrimPrefix(srv.URL, "http")
}

// connect opens a connection, authenticated as u (NIP-42) unless u is nil.
func connect(t *testing.T, url string, u *user) *nostr.Relay {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	r, err := nostr.RelayConnect(ctx, url)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { r.Close() })
	if u == nil {
		return r
	}
	// The challenge arrives right after connecting: retry until it's there.
	for {
		err = r.Auth(ctx, func(ev *nostr.Event) error { return ev.Sign(u.sk) })
		if err == nil {
			return r
		}
		if ctx.Err() != nil {
			t.Fatalf("auth: %v", err)
		}
		time.Sleep(20 * time.Millisecond)
	}
}

func event(u user, kind int, tags nostr.Tags, at time.Time) nostr.Event {
	ev := nostr.Event{Kind: kind, Tags: tags, CreatedAt: nostr.Timestamp(at.Unix()), Content: "x"}
	_ = ev.Sign(u.sk)
	return ev
}

func publish(t *testing.T, r *nostr.Relay, ev nostr.Event) error {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	return r.Publish(ctx, ev)
}

func TestWritePolicy(t *testing.T) {
	alice, bob, stranger, ephemeral := newUser(), newUser(), newUser(), newUser()
	url := startRelay(t, alice, bob)
	r := connect(t, url, nil)
	now := time.Now()

	accepted := map[string]nostr.Event{
		"local profile":           event(alice, nostr.KindProfileMetadata, nil, now),
		"local relay list":        event(alice, nostr.KindRelayListMetadata, nostr.Tags{{"r", "wss://x"}}, now),
		"local DM relay list":     event(bob, nostr.KindDMRelayList, nostr.Tags{{"relay", "wss://x"}}, now),
		"gift wrap to local user": event(ephemeral, nostr.KindGiftWrap, nostr.Tags{{"p", bob.pk}}, now.Add(-24*time.Hour)),
	}
	for name, ev := range accepted {
		if err := publish(t, r, ev); err != nil {
			t.Errorf("%s: rejected: %v", name, err)
		}
	}

	rejected := map[string]nostr.Event{
		"stranger profile":        event(stranger, nostr.KindProfileMetadata, nil, now),
		"gift wrap to non-local":  event(ephemeral, nostr.KindGiftWrap, nostr.Tags{{"p", stranger.pk}}, now),
		"bare rumor (kind 14)":    event(alice, nostr.KindDirectMessage, nostr.Tags{{"p", bob.pk}}, now),
		"text note (kind 1)":      event(alice, nostr.KindTextNote, nil, now),
		"1 hour in the future":    event(alice, nostr.KindProfileMetadata, nil, now.Add(time.Hour)),
		"gift wrap without p tag": event(ephemeral, nostr.KindGiftWrap, nil, now),
	}
	for name, ev := range rejected {
		if err := publish(t, r, ev); err == nil {
			t.Errorf("%s: accepted, want rejected", name)
		}
	}
}

// query subscribes with filter and returns the stored events, or the CLOSED reason.
func query(t *testing.T, r *nostr.Relay, filter nostr.Filter) (evs []*nostr.Event, closed string) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	sub, err := r.Subscribe(ctx, nostr.Filters{filter})
	if err != nil {
		t.Fatal(err)
	}
	defer sub.Unsub()
	for {
		select {
		case ev := <-sub.Events:
			evs = append(evs, ev)
		case <-sub.EndOfStoredEvents:
			return evs, ""
		case reason := <-sub.ClosedReason:
			return evs, reason
		case <-ctx.Done():
			t.Fatal("no EOSE nor CLOSED")
		}
	}
}

func TestGiftWrapsOnlyServedToRecipient(t *testing.T) {
	bob, carol, ephemeral := newUser(), newUser(), newUser()
	url := startRelay(t, bob, carol)
	gift := event(ephemeral, nostr.KindGiftWrap, nostr.Tags{{"p", bob.pk}}, time.Now())
	if err := publish(t, connect(t, url, nil), gift); err != nil {
		t.Fatal(err)
	}
	forBob := nostr.Filter{Kinds: []int{nostr.KindGiftWrap}, Tags: nostr.TagMap{"p": {bob.pk}}}

	if evs, closed := query(t, connect(t, url, &bob), forBob); len(evs) != 1 || closed != "" {
		t.Errorf("bob: got %d events, closed %q; want 1", len(evs), closed)
	}

	cases := []struct {
		name   string
		u      *user
		filter nostr.Filter
		want   string // CLOSED reason prefix
	}{
		{"anonymous", nil, forBob, "auth-required:"},
		{"carol asks for bob's", &carol, forBob, "restricted:"},
		{"carol, no #p", &carol, nostr.Filter{Kinds: []int{nostr.KindGiftWrap}}, "restricted:"},
		{"carol, no kinds", &carol, nostr.Filter{Tags: nostr.TagMap{"p": {bob.pk}}}, "restricted:"},
		{"carol, bob and herself", &carol, nostr.Filter{Kinds: []int{nostr.KindGiftWrap}, Tags: nostr.TagMap{"p": {carol.pk, bob.pk}}}, "restricted:"},
	}
	for _, c := range cases {
		evs, closed := query(t, connect(t, url, c.u), c.filter)
		if len(evs) != 0 || !strings.HasPrefix(closed, c.want) {
			t.Errorf("%s: got %d events, closed %q; want 0, %q", c.name, len(evs), closed, c.want)
		}
	}

	// Profiles stay public, no auth needed.
	if _, closed := query(t, connect(t, url, nil), nostr.Filter{Kinds: []int{nostr.KindProfileMetadata}}); closed != "" {
		t.Errorf("anonymous profile query closed: %q", closed)
	}
}
