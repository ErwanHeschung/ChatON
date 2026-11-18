package presence

import (
	"testing"
	"time"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/khatru"
)

// A signal lasts 150 s per connection, and goes with its connection.
func TestMemory(t *testing.T) {
	s := New()
	now := time.Now()
	s.Now = func() time.Time { return now }
	key := nostr.Generate()
	ev := nostr.Event{Kind: Kind, Tags: nostr.Tags{{"status", "online"}}, CreatedAt: nostr.Now()}
	_ = ev.Sign(key)
	phone, laptop := &khatru.WebSocket{}, &khatru.WebSocket{}
	s.put(phone, ev)
	s.put(laptop, ev)
	f := nostr.Filter{Kinds: []nostr.Kind{Kind}, Authors: []nostr.PubKey{ev.PubKey}}
	if n := len(s.Query(f)); n != 2 {
		t.Fatalf("two devices: %d", n)
	}
	s.drop(phone)
	if n := len(s.Query(f)); n != 1 {
		t.Fatalf("after the phone left: %d", n)
	}
	now = now.Add(TTL - time.Second)
	if n := len(s.Query(f)); n != 1 {
		t.Fatalf("just before expiry: %d", n)
	}
	now = now.Add(2 * time.Second)
	if n := len(s.Query(f)); n != 0 {
		t.Fatalf("expired: %d", n)
	}
	if len(s.byKey) != 0 {
		t.Fatal("expired keys stay in memory")
	}
}
