package directory

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"fiatjaf.com/nostr"
	"go.etcd.io/bbolt"
)

const publicURL = "https://chaton.test"

var now = time.Unix(1700000000, 0)

func newDirectory(t *testing.T) (*Directory, *http.ServeMux) {
	t.Helper()
	db, err := bbolt.Open(filepath.Join(t.TempDir(), "t.bolt"), 0600, nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	d, err := New(db, publicURL)
	if err != nil {
		t.Fatal(err)
	}
	d.now = func() time.Time { return now }
	mux := http.NewServeMux()
	d.Register(mux)
	return d, mux
}

type auth struct {
	url, method string
	payload     []byte // nil: hash of the real body
	createdAt   time.Time
	signWith    *nostr.SecretKey // nil: the claimer's key
}

func claim(t *testing.T, mux *http.ServeMux, sk nostr.SecretKey, body string, a auth) *httptest.ResponseRecorder {
	t.Helper()
	if a.url == "" {
		a.url = publicURL + ClaimPath
	}
	if a.method == "" {
		a.method = "POST"
	}
	if a.payload == nil {
		a.payload = []byte(body)
	}
	if a.createdAt.IsZero() {
		a.createdAt = now
	}
	sum := sha256.Sum256(a.payload)
	ev := nostr.Event{
		Kind:      nostr.KindHTTPAuth,
		CreatedAt: nostr.Timestamp(a.createdAt.Unix()),
		Tags:      nostr.Tags{{"u", a.url}, {"method", a.method}, {"payload", hex.EncodeToString(sum[:])}},
	}
	if err := ev.Sign(sk); err != nil {
		t.Fatal(err)
	}
	if a.signWith != nil { // tamper: signature by another key, pubkey kept
		pk := ev.PubKey
		_ = ev.Sign(*a.signWith)
		ev.PubKey = pk
	}
	raw, _ := json.Marshal(ev)
	req := httptest.NewRequest("POST", ClaimPath, strings.NewReader(body))
	req.Header.Set("Authorization", "Nostr "+base64.StdEncoding.EncodeToString(raw))
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	return rec
}

func lookup(t *testing.T, mux *http.ServeMux, name string) nip05Response {
	t.Helper()
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest("GET", "/.well-known/nostr.json?name="+name, nil))
	if got := rec.Header().Get("Access-Control-Allow-Origin"); got != "*" {
		t.Fatalf("CORS header = %q", got)
	}
	var resp nip05Response
	if err := json.Unmarshal(rec.Body.Bytes(), &resp); err != nil {
		t.Fatal(err)
	}
	return resp
}

func TestClaimThenLookup(t *testing.T) {
	d, mux := newDirectory(t)
	alice := nostr.Generate()
	alicePK := alice.Public()

	if rec := claim(t, mux, alice, `{"name":"Alice"}`, auth{}); rec.Code != http.StatusCreated {
		t.Fatalf("claim: %d %s", rec.Code, rec.Body)
	}
	resp := lookup(t, mux, "ALICE")
	if resp.Names["alice"] != alicePK.Hex() || resp.Relays[alicePK.Hex()][0] != "wss://chaton.test" {
		t.Fatalf("lookup: %+v", resp)
	}
	if !d.IsLocal(alicePK) {
		t.Fatal("alice should be local")
	}
	if len(lookup(t, mux, "nobody").Names) != 0 {
		t.Fatal("unknown name should return no names")
	}
}

func TestClaimConflictsAndValidation(t *testing.T) {
	_, mux := newDirectory(t)
	alice, bob := nostr.Generate(), nostr.Generate()
	claim(t, mux, alice, `{"name":"alice"}`, auth{})

	cases := []struct {
		name string
		sk   nostr.SecretKey
		body string
		want int
	}{
		{"name taken", bob, `{"name":"alice"}`, http.StatusConflict},
		{"key already has a handle", alice, `{"name":"alice2"}`, http.StatusConflict},
		{"too short", bob, `{"name":"ab"}`, http.StatusBadRequest},
		{"space", bob, `{"name":"a b c"}`, http.StatusBadRequest},
		{"instance key", bob, `{"name":"_"}`, http.StatusBadRequest},
		{"missing name", bob, `{}`, http.StatusBadRequest},
	}
	for _, c := range cases {
		if rec := claim(t, mux, c.sk, c.body, auth{}); rec.Code != c.want {
			t.Errorf("%s: got %d, want %d (%s)", c.name, rec.Code, c.want, rec.Body)
		}
	}
}

func TestClaimRejectsBadNIP98(t *testing.T) {
	_, mux := newDirectory(t)
	sk := nostr.Generate()
	other := nostr.Generate()
	body := `{"name":"carol"}`

	cases := map[string]auth{
		"wrong u":       {url: "https://evil.test" + ClaimPath},
		"wrong method":  {method: "GET"},
		"wrong payload": {payload: []byte(`{"name":"mallory"}`)},
		"expired":       {createdAt: now.Add(-2 * time.Minute)},
		"future":        {createdAt: now.Add(2 * time.Minute)},
		"bad signature": {signWith: &other},
	}
	for name, a := range cases {
		if rec := claim(t, mux, sk, body, a); rec.Code != http.StatusUnauthorized {
			t.Errorf("%s: got %d, want 401", name, rec.Code)
		}
	}

	req := httptest.NewRequest("POST", ClaimPath, strings.NewReader(body))
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, req)
	if rec.Code != http.StatusUnauthorized {
		t.Errorf("missing header: got %d, want 401", rec.Code)
	}
}
