// Package push wakes the phones of local users when a DM arrives
// (spec/federation.md, "Notifications push"): WebPush (RFC 8030/8291/8292) to
// the endpoints their devices registered, e.g. through UnifiedPush.
package push

import (
	"context"
	"crypto/ecdh"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"os"
	"strings"
	"sync"
	"syscall"
	"time"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/khatru"
	"github.com/SherClockHolmes/webpush-go"
	"go.etcd.io/bbolt"

	"github.com/ErwanHeschung/ChatON/server/internal/handlers"
	"github.com/ErwanHeschung/ChatON/server/internal/models/dto"
	"github.com/ErwanHeschung/ChatON/server/internal/nip98"
)

const (
	Path         = "/api/push"
	MaxEndpoints = 10 // per key; the oldest goes first
	maxEndpoint  = 1000
	ttl          = 24 * 60 * 60
)

// MinInterval between two pushes to one endpoint (a var for tests).
var MinInterval = 10 * time.Second

// The push content: constant, the phone fetches the messages itself.
var payload = []byte(`{"v":0}`)

// bucket: pubkey (32 bytes) + endpoint → subscription JSON.
var bucket = []byte("push")

type Directory interface {
	IsLocal(pubkey nostr.PubKey) bool
}

type subscription struct {
	Endpoint string `json:"endpoint"`
	P256dh   string `json:"p256dh"`
	Auth     string `json:"auth"`
	Added    int64  `json:"added,omitempty"`
}

type Push struct {
	db           *bbolt.DB
	dir          Directory
	publicURL    string
	vapidPrivate string // base64url, raw P-256 scalar
	vapidPublic  string // base64url, uncompressed point
	allowPrivate bool   // dev only: endpoints on private IPs and plain http
	client       *http.Client
	now          func() time.Time

	mu   sync.Mutex
	last map[string]time.Time // endpoint → last push
}

func New(db *bbolt.DB, dir Directory, publicURL string, vapid *ecdh.PrivateKey, allowPrivate bool) (*Push, error) {
	if err := db.Update(func(tx *bbolt.Tx) error {
		_, err := tx.CreateBucketIfNotExists(bucket)
		return err
	}); err != nil {
		return nil, fmt.Errorf("push bucket: %w", err)
	}
	return &Push{
		db:           db,
		dir:          dir,
		publicURL:    strings.TrimSuffix(publicURL, "/"),
		vapidPrivate: base64.RawURLEncoding.EncodeToString(vapid.Bytes()),
		vapidPublic:  base64.RawURLEncoding.EncodeToString(vapid.PublicKey().Bytes()),
		allowPrivate: allowPrivate,
		client:       safeClient(allowPrivate),
		now:          time.Now,
		last:         map[string]time.Time{},
	}, nil
}

// LoadVAPIDKey reads the VAPID key from b64 (SERVER_VAPIDKEY), else from path,
// creating it there on first start (like relay.key).
func LoadVAPIDKey(b64, path string) (*ecdh.PrivateKey, error) {
	if b64 == "" {
		raw, err := os.ReadFile(path)
		switch {
		case err == nil:
			b64 = strings.TrimSpace(string(raw))
		case errors.Is(err, os.ErrNotExist):
			key, err := ecdh.P256().GenerateKey(rand.Reader)
			if err != nil {
				return nil, fmt.Errorf("vapid key: %w", err)
			}
			enc := base64.RawURLEncoding.EncodeToString(key.Bytes())
			if err := os.WriteFile(path, []byte(enc+"\n"), 0o600); err != nil {
				return nil, fmt.Errorf("vapid key: %w", err)
			}
			return key, nil
		default:
			return nil, fmt.Errorf("vapid key: %w", err)
		}
	}
	raw, err := base64.RawURLEncoding.DecodeString(b64)
	if err != nil {
		return nil, fmt.Errorf("vapid key: %w", err)
	}
	return ecdh.P256().NewPrivateKey(raw)
}

// Bind adds pushes after the relay's own OnEventSaved (communities).
func (p *Push) Bind(rl *khatru.Relay) {
	before := rl.OnEventSaved
	rl.OnEventSaved = func(ctx context.Context, ev nostr.Event) {
		if before != nil {
			before(ctx, ev)
		}
		p.Saved(ctx, ev)
	}
}

func (p *Push) Register(mux *http.ServeMux) {
	mux.HandleFunc("GET "+Path, func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]string{"vapid": p.vapidPublic})
	})
	mux.HandleFunc("POST "+Path, p.subscribe)
	mux.HandleFunc("DELETE "+Path, p.unsubscribe)
}

// authed reads the body and checks its NIP-98 signature by a local key.
func (p *Push) authed(w http.ResponseWriter, r *http.Request) (nostr.PubKey, subscription, bool) {
	var sub subscription
	body, err := io.ReadAll(io.LimitReader(r.Body, 4096))
	if err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Unreadable body", err))
		return nostr.ZeroPK, sub, false
	}
	pubkey, err := nip98.Verify(r, p.publicURL+Path, body, p.now())
	if err != nil {
		handlers.HandleError(w, dto.ErrUnauthorized("Invalid NIP-98 authorization", err))
		return nostr.ZeroPK, sub, false
	}
	if !p.dir.IsLocal(pubkey) {
		handlers.HandleError(w, dto.ErrForbidden("Claim a handle on this instance first", nil))
		return nostr.ZeroPK, sub, false
	}
	if err := json.Unmarshal(body, &sub); err != nil || sub.Endpoint == "" || len(sub.Endpoint) > maxEndpoint {
		handlers.HandleError(w, dto.ErrInvalidInput("Expected {endpoint, p256dh, auth}", err))
		return nostr.ZeroPK, sub, false
	}
	return pubkey, sub, true
}

func (p *Push) subscribe(w http.ResponseWriter, r *http.Request) {
	pubkey, sub, ok := p.authed(w, r)
	if !ok {
		return
	}
	if err := p.checkEndpoint(r.Context(), sub.Endpoint); err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Endpoint refused", err))
		return
	}
	dh, err1 := base64.RawURLEncoding.DecodeString(strings.TrimRight(sub.P256dh, "="))
	auth, err2 := base64.RawURLEncoding.DecodeString(strings.TrimRight(sub.Auth, "="))
	if err1 != nil || err2 != nil || len(dh) != 65 || len(auth) != 16 {
		handlers.HandleError(w, dto.ErrInvalidInput("p256dh (65 bytes) and auth (16 bytes) in base64url", nil))
		return
	}
	sub.Added = p.now().UnixNano()
	if err := p.put(pubkey, sub); err != nil {
		handlers.HandleError(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

func (p *Push) unsubscribe(w http.ResponseWriter, r *http.Request) {
	pubkey, sub, ok := p.authed(w, r)
	if !ok {
		return
	}
	if err := p.delete(pubkey, sub.Endpoint); err != nil {
		handlers.HandleError(w, err)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// Saved is the relay's OnEventSaved part for pushes: a gift wrap for a local
// user with endpoints, unless their own device published it (the self copy).
func (p *Push) Saved(ctx context.Context, ev nostr.Event) {
	if ev.Kind != nostr.KindGiftWrap {
		return
	}
	tag := ev.Tags.Find("p")
	if len(tag) < 2 {
		return
	}
	pubkey, err := nostr.PubKeyFromHex(tag[1])
	if err != nil || khatru.IsAuthed(ctx, pubkey) {
		return
	}
	for _, sub := range p.list(pubkey) {
		if p.due(sub.Endpoint) {
			go p.send(pubkey, sub)
		}
	}
}

// due rate-limits each endpoint (the push service keeps the latest anyway: Topic).
// ponytail: the map holds every endpoint ever pushed to; prune when it matters.
func (p *Push) due(endpoint string) bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	now := p.now()
	if now.Sub(p.last[endpoint]) < MinInterval {
		return false
	}
	p.last[endpoint] = now
	return true
}

func (p *Push) send(pubkey nostr.PubKey, sub subscription) {
	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Second)
	defer cancel()
	resp, err := webpush.SendNotificationWithContext(ctx, payload, &webpush.Subscription{
		Endpoint: sub.Endpoint,
		Keys:     webpush.Keys{P256dh: sub.P256dh, Auth: sub.Auth},
	}, &webpush.Options{
		HTTPClient:      p.client,
		Subscriber:      p.publicURL,
		Topic:           "chaton",
		TTL:             ttl,
		Urgency:         webpush.UrgencyHigh,
		VAPIDPublicKey:  p.vapidPublic,
		VAPIDPrivateKey: p.vapidPrivate,
	})
	if err != nil {
		log.Printf("push: %v", err)
		return
	}
	_ = resp.Body.Close()
	switch resp.StatusCode {
	case http.StatusNotFound, http.StatusGone: // unsubscribed (RFC 8030)
		if err := p.delete(pubkey, sub.Endpoint); err != nil {
			log.Printf("push: forget endpoint: %v", err)
		}
	default:
		if resp.StatusCode >= 300 {
			log.Printf("push: %s answered %d", hostOf(sub.Endpoint), resp.StatusCode)
		}
	}
}

func (p *Push) put(pubkey nostr.PubKey, sub subscription) error {
	value, err := json.Marshal(sub)
	if err != nil {
		return err
	}
	return p.db.Update(func(tx *bbolt.Tx) error {
		b := tx.Bucket(bucket)
		subs := entries(b, pubkey)
		// Over the cap: the oldest go (re-registering the same endpoint just refreshes it).
		for len(subs) >= MaxEndpoints {
			oldest := 0
			for i, s := range subs {
				if s.Added < subs[oldest].Added {
					oldest = i
				}
			}
			if subs[oldest].Endpoint == sub.Endpoint {
				break
			}
			if err := b.Delete(key(pubkey, subs[oldest].Endpoint)); err != nil {
				return err
			}
			subs = append(subs[:oldest], subs[oldest+1:]...)
		}
		return b.Put(key(pubkey, sub.Endpoint), value)
	})
}

func (p *Push) delete(pubkey nostr.PubKey, endpoint string) error {
	return p.db.Update(func(tx *bbolt.Tx) error {
		return tx.Bucket(bucket).Delete(key(pubkey, endpoint))
	})
}

func (p *Push) list(pubkey nostr.PubKey) (subs []subscription) {
	_ = p.db.View(func(tx *bbolt.Tx) error {
		subs = entries(tx.Bucket(bucket), pubkey)
		return nil
	})
	return subs
}

func entries(b *bbolt.Bucket, pubkey nostr.PubKey) []subscription {
	var subs []subscription
	c := b.Cursor()
	for k, v := c.Seek(pubkey[:]); k != nil && len(k) > 32 && string(k[:32]) == string(pubkey[:]); k, v = c.Next() {
		var s subscription
		if json.Unmarshal(v, &s) == nil {
			subs = append(subs, s)
		}
	}
	return subs
}

func key(pubkey nostr.PubKey, endpoint string) []byte {
	return append(pubkey[:], endpoint...)
}

// checkEndpoint: https on a public address (UnifiedPush server spec). The dialer
// checks the address again on every push, after DNS.
func (p *Push) checkEndpoint(ctx context.Context, endpoint string) error {
	u, err := url.Parse(endpoint)
	if err != nil || u.Host == "" {
		return errors.New("not a URL")
	}
	if u.Scheme != "https" && !(p.allowPrivate && u.Scheme == "http") {
		return errors.New("https only")
	}
	if p.allowPrivate {
		return nil
	}
	addrs, err := net.DefaultResolver.LookupNetIP(ctx, "ip", u.Hostname())
	if err != nil {
		return fmt.Errorf("resolve: %w", err)
	}
	for _, a := range addrs {
		if !public(a) {
			return errors.New("not a public address")
		}
	}
	return nil
}

var cgnat = netip.MustParsePrefix("100.64.0.0/10")

func public(a netip.Addr) bool {
	a = a.Unmap()
	return a.IsGlobalUnicast() && !a.IsPrivate() && !cgnat.Contains(a)
}

// safeClient refuses to connect to non-public addresses (no SSRF through
// endpoints), and doesn't follow redirects.
func safeClient(allowPrivate bool) *http.Client {
	dialer := &net.Dialer{
		Timeout: 10 * time.Second,
		Control: func(_, address string, _ syscall.RawConn) error {
			if allowPrivate {
				return nil
			}
			ap, err := netip.ParseAddrPort(address)
			if err != nil || !public(ap.Addr()) {
				return fmt.Errorf("push endpoint %s is not a public address", address)
			}
			return nil
		},
	}
	return &http.Client{
		Timeout:       20 * time.Second,
		Transport:     &http.Transport{DialContext: dialer.DialContext, TLSHandshakeTimeout: 10 * time.Second},
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}
}

func hostOf(endpoint string) string {
	if u, err := url.Parse(endpoint); err == nil {
		return u.Host
	}
	return "?"
}
