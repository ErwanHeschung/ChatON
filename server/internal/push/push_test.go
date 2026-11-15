package push_test

import (
	"bytes"
	"context"
	"crypto/aes"
	"crypto/cipher"
	"crypto/ecdh"
	"crypto/hkdf"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore/boltdb"

	"github.com/ErwanHeschung/ChatON/server/internal/community"
	"github.com/ErwanHeschung/ChatON/server/internal/push"
	"github.com/ErwanHeschung/ChatON/server/internal/relay"
)

type fakeDir map[nostr.PubKey]bool

func (d fakeDir) IsLocal(pk nostr.PubKey) bool { return d[pk] }

// device is a phone: its WebPush keys, and an endpoint that collects pushes.
type device struct {
	key    *ecdh.PrivateKey
	auth   []byte
	url    string
	status int // answer to pushes; 0 = 201

	mu     sync.Mutex
	pushes [][]byte
}

func newDevice(t *testing.T) *device {
	t.Helper()
	key, _ := ecdh.P256().GenerateKey(rand.Reader)
	d := &device{key: key, auth: make([]byte, 16)}
	_, _ = rand.Read(d.auth)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		if r.Header.Get("Content-Encoding") != "aes128gcm" || r.Header.Get("Topic") != "chaton" ||
			!strings.HasPrefix(r.Header.Get("Authorization"), "vapid ") {
			t.Errorf("push headers: %v", r.Header)
		}
		d.mu.Lock()
		d.pushes = append(d.pushes, body)
		status := d.status
		d.mu.Unlock()
		if status == 0 {
			status = http.StatusCreated
		}
		w.WriteHeader(status)
	}))
	t.Cleanup(srv.Close)
	d.url = srv.URL + "/up/" + hex.EncodeToString(d.auth)
	return d
}

func (d *device) body() map[string]string {
	b64 := base64.RawURLEncoding.EncodeToString
	return map[string]string{"endpoint": d.url, "p256dh": b64(d.key.PublicKey().Bytes()), "auth": b64(d.auth)}
}

func (d *device) setStatus(status int) {
	d.mu.Lock()
	defer d.mu.Unlock()
	d.status = status
}

func (d *device) count() int {
	d.mu.Lock()
	defer d.mu.Unlock()
	return len(d.pushes)
}

// decrypt opens an aes128gcm WebPush record (RFC 8291) as the device would.
func (d *device) decrypt(t *testing.T, record []byte) []byte {
	t.Helper()
	salt, idlen := record[:16], int(record[20])
	serverPub, ciphertext := record[21:21+idlen], record[21+idlen:]
	pub, err := ecdh.P256().NewPublicKey(serverPub)
	if err != nil {
		t.Fatal(err)
	}
	secret, _ := d.key.ECDH(pub)
	prkKey, _ := hkdf.Extract(sha256.New, secret, d.auth)
	info := append(append([]byte("WebPush: info\x00"), d.key.PublicKey().Bytes()...), serverPub...)
	ikm, _ := hkdf.Expand(sha256.New, prkKey, string(info), 32)
	prk, _ := hkdf.Extract(sha256.New, ikm, salt)
	cek, _ := hkdf.Expand(sha256.New, prk, "Content-Encoding: aes128gcm\x00", 16)
	nonce, _ := hkdf.Expand(sha256.New, prk, "Content-Encoding: nonce\x00", 12)
	block, _ := aes.NewCipher(cek)
	gcm, _ := cipher.NewGCM(block)
	plain, err := gcm.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		t.Fatalf("decrypt: %v", err)
	}
	plain = bytes.TrimRight(plain, "\x00")
	return plain[:len(plain)-1] // last record: delimiter 0x02
}

type instance struct {
	http string // http://127.0.0.1:port
	ws   string
}

func serve(t *testing.T, allowPrivate bool, locals ...nostr.SecretKey) instance {
	t.Helper()
	db := &boltdb.BoltBackend{Path: filepath.Join(t.TempDir(), "r.bolt")}
	if err := db.Init(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(db.Close)
	dir := fakeDir{}
	for _, sk := range locals {
		dir[sk.Public()] = true
	}
	groups, err := community.New(nostr.Generate(), "ws://test", db, dir)
	if err != nil {
		t.Fatal(err)
	}
	rl := relay.New(relay.Config{Name: "test"}, db, dir, groups)
	srv := httptest.NewUnstartedServer(rl)
	srv.Start()
	t.Cleanup(srv.Close)
	vapid, err := push.LoadVAPIDKey("", filepath.Join(t.TempDir(), "vapid.key"))
	if err != nil {
		t.Fatal(err)
	}
	p, err := push.New(db.DB, dir, srv.URL, vapid, allowPrivate)
	if err != nil {
		t.Fatal(err)
	}
	p.Bind(rl)
	p.Register(rl.Router())
	return instance{srv.URL, "ws" + strings.TrimPrefix(srv.URL, "http")}
}

// call sends a NIP-98 signed request (unsigned when sk is nil).
func call(t *testing.T, in instance, method string, sk *nostr.SecretKey, body any) int {
	t.Helper()
	raw, _ := json.Marshal(body)
	req, _ := http.NewRequest(method, in.http+push.Path, bytes.NewReader(raw))
	if sk != nil {
		sum := sha256.Sum256(raw)
		ev := nostr.Event{Kind: nostr.KindHTTPAuth, CreatedAt: nostr.Now(), Tags: nostr.Tags{
			{"u", in.http + push.Path}, {"method", method}, {"payload", hex.EncodeToString(sum[:])},
		}}
		_ = ev.Sign(*sk)
		j, _ := json.Marshal(ev)
		req.Header.Set("Authorization", "Nostr "+base64.StdEncoding.EncodeToString(j))
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	_ = resp.Body.Close()
	return resp.StatusCode
}

// giftWrap for bob, published over a connection authenticated as `as` (nil: anonymous).
func giftWrap(t *testing.T, in instance, to nostr.PubKey, as *nostr.SecretKey) {
	t.Helper()
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	r, err := nostr.RelayConnect(ctx, in.ws, nostr.RelayOptions{})
	if err != nil {
		t.Fatal(err)
	}
	defer r.Close()
	for as != nil {
		err := r.Auth(ctx, func(_ context.Context, ev *nostr.Event) error { return ev.Sign(*as) })
		if err == nil {
			break
		}
		if ctx.Err() != nil {
			t.Fatalf("auth: %v", err)
		}
		time.Sleep(20 * time.Millisecond)
	}
	ev := nostr.Event{Kind: nostr.KindGiftWrap, CreatedAt: nostr.Now(), Tags: nostr.Tags{{"p", to.Hex()}}, Content: "opaque"}
	_ = ev.Sign(nostr.Generate())
	if err := r.Publish(ctx, ev); err != nil {
		t.Fatal(err)
	}
}

func eventually(t *testing.T, what string, ok func() bool) {
	t.Helper()
	for range 100 {
		if ok() {
			return
		}
		time.Sleep(20 * time.Millisecond)
	}
	t.Fatal(what)
}

func TestPush(t *testing.T) {
	bob, stranger := nostr.Generate(), nostr.Generate()
	in := serve(t, true, bob)
	phone := newDevice(t)

	resp, err := http.Get(in.http + push.Path)
	if err != nil {
		t.Fatal(err)
	}
	var info struct{ Vapid string }
	_ = json.NewDecoder(resp.Body).Decode(&info)
	_ = resp.Body.Close()
	if raw, _ := base64.RawURLEncoding.DecodeString(info.Vapid); len(raw) != 65 {
		t.Fatalf("vapid public key: %q", info.Vapid)
	}

	if code := call(t, in, "POST", nil, phone.body()); code != 401 {
		t.Errorf("unsigned: %d", code)
	}
	if code := call(t, in, "POST", &stranger, phone.body()); code != 403 {
		t.Errorf("no handle here: %d", code)
	}
	bad := phone.body()
	bad["p256dh"] = "short"
	if code := call(t, in, "POST", &bob, bad); code != 400 {
		t.Errorf("bad keys: %d", code)
	}
	if code := call(t, in, "POST", &bob, phone.body()); code != 204 {
		t.Fatalf("register: %d", code)
	}

	// A DM for bob: one encrypted push with the constant payload.
	giftWrap(t, in, bob.Public(), nil)
	eventually(t, "no push", func() bool { return phone.count() == 1 })
	if got := phone.decrypt(t, phone.pushes[0]); string(got) != `{"v":0}` {
		t.Errorf("payload %q", got)
	}

	// Rate limit: a second DM right away doesn't push again.
	giftWrap(t, in, bob.Public(), nil)
	time.Sleep(200 * time.Millisecond)
	if n := phone.count(); n != 1 {
		t.Errorf("rate limit: %d pushes", n)
	}

	// The self copy, from bob's own authenticated device: no push either way.
	other := newDevice(t)
	if code := call(t, in, "POST", &bob, other.body()); code != 204 {
		t.Fatal(code)
	}
	giftWrap(t, in, bob.Public(), &bob)
	time.Sleep(200 * time.Millisecond)
	if n := other.count(); n != 0 {
		t.Errorf("self copy pushed: %d", n)
	}

	// 410 from the push service: the endpoint is forgotten.
	defer func(was time.Duration) { push.MinInterval = was }(push.MinInterval)
	push.MinInterval = 0
	other.setStatus(http.StatusGone)
	giftWrap(t, in, bob.Public(), nil)
	eventually(t, "no push to other", func() bool { return other.count() == 1 })
	other.setStatus(0)
	time.Sleep(100 * time.Millisecond) // the 410 handling runs after the response
	giftWrap(t, in, bob.Public(), nil)
	eventually(t, "no second push to phone", func() bool { return phone.count() == 2 })
	if n := other.count(); n != 1 {
		t.Errorf("gone endpoint still pushed: %d", n)
	}

	// Unregister.
	if code := call(t, in, "DELETE", &bob, map[string]string{"endpoint": phone.url}); code != 204 {
		t.Errorf("unregister: %d", code)
	}
}

func TestEndpointRules(t *testing.T) {
	bob := nostr.Generate()
	in := serve(t, false, bob)
	phone := newDevice(t) // on 127.0.0.1
	body := phone.body()
	if code := call(t, in, "POST", &bob, body); code != 400 {
		t.Errorf("plain http: %d", code)
	}
	body["endpoint"] = "https://127.0.0.1/up/x"
	if code := call(t, in, "POST", &bob, body); code != 400 {
		t.Errorf("loopback: %d", code)
	}
	body["endpoint"] = "https://10.1.2.3/up/x"
	if code := call(t, in, "POST", &bob, body); code != 400 {
		t.Errorf("private: %d", code)
	}
	body["endpoint"] = "https://" + strings.Repeat("a", 1000) + ".example/"
	if code := call(t, in, "POST", &bob, body); code != 400 {
		t.Errorf("too long: %d", code)
	}
}

func TestOldestEndpointGoes(t *testing.T) {
	bob := nostr.Generate()
	in := serve(t, true, bob)
	var devices []*device
	for i := range push.MaxEndpoints + 1 {
		d := newDevice(t)
		devices = append(devices, d)
		if code := call(t, in, "POST", &bob, d.body()); code != 204 {
			t.Fatalf("register %d: %d", i, code)
		}
	}
	giftWrap(t, in, bob.Public(), nil)
	eventually(t, "newest not pushed", func() bool { return devices[push.MaxEndpoints].count() == 1 })
	time.Sleep(200 * time.Millisecond)
	if n := devices[0].count(); n != 0 {
		t.Errorf("oldest still registered: %d", n)
	}
	for i, d := range devices[1:] {
		if d.count() != 1 {
			t.Errorf("device %d: %d pushes", i+1, d.count())
		}
	}
}
