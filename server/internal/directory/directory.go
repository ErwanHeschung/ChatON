// Package directory maps pseudo@instance to a public key (NIP-05) and lets a
// key claim a free handle with a NIP-98 signed request. See spec/federation.md.
package directory

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net/http"
	"regexp"
	"strings"
	"time"

	"github.com/go-playground/validator/v10"
	"github.com/jmoiron/sqlx"
	"github.com/mattn/go-sqlite3"

	"github.com/ErwanHeschung/ChatON/server/internal/handlers"
	"github.com/ErwanHeschung/ChatON/server/internal/models/dto"
)

const ClaimPath = "/api/handle"

var nameRe = regexp.MustCompile(`^[a-z0-9._-]{3,32}$`) // also rules out "_" (NIP-05 instance key)

type Directory struct {
	db        *sqlx.DB
	publicURL string // https://chaton.example
	relayURL  string // wss://chaton.example
	validate  *validator.Validate
	now       func() time.Time
}

func New(db *sqlx.DB, publicURL string) (*Directory, error) {
	_, err := db.Exec(`CREATE TABLE IF NOT EXISTS handles (
		name TEXT PRIMARY KEY,
		pubkey TEXT NOT NULL UNIQUE,
		created_at INTEGER NOT NULL)`)
	if err != nil {
		return nil, err
	}
	publicURL = strings.TrimSuffix(publicURL, "/")
	relayURL := "ws" + strings.TrimPrefix(publicURL, "http") // http→ws, https→wss
	return &Directory{db, publicURL, relayURL, validator.New(), time.Now}, nil
}

// IsLocal reports whether pubkey holds a handle on this instance. Fails closed.
func (d *Directory) IsLocal(pubkey string) bool {
	var n int
	if err := d.db.Get(&n, `SELECT count(*) FROM handles WHERE pubkey = ?`, pubkey); err != nil {
		log.Printf("directory: IsLocal: %v", err)
		return false
	}
	return n > 0
}

func (d *Directory) Register(mux *http.ServeMux) {
	mux.HandleFunc("GET /.well-known/nostr.json", d.nostrJSON)
	mux.HandleFunc("POST "+ClaimPath, d.claim)
}

type nip05Response struct {
	Names  map[string]string   `json:"names"`
	Relays map[string][]string `json:"relays"`
}

func (d *Directory) nostrJSON(w http.ResponseWriter, r *http.Request) {
	name := strings.ToLower(r.URL.Query().Get("name"))
	resp := nip05Response{Names: map[string]string{}, Relays: map[string][]string{}}
	var pubkey string
	err := d.db.Get(&pubkey, `SELECT pubkey FROM handles WHERE name = ?`, name)
	if err == nil {
		resp.Names[name] = pubkey
		resp.Relays[pubkey] = []string{d.relayURL}
	}
	w.Header().Set("Access-Control-Allow-Origin", "*") // required by NIP-05
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(resp)
}

type claimRequest struct {
	Name string `json:"name" validate:"required"`
}

type claimResponse struct {
	Name   string `json:"name"`
	PubKey string `json:"pubkey"`
}

func (d *Directory) claim(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(io.LimitReader(r.Body, 1024))
	if err != nil {
		handlers.HandleError(w, dto.ErrInvalidInput("Unreadable body", err))
		return
	}
	pubkey, err := verifyNIP98(r, d.publicURL+ClaimPath, body, d.now())
	if err != nil {
		handlers.HandleError(w, dto.ErrUnauthorized("Invalid NIP-98 authorization", err))
		return
	}

	r.Body = io.NopCloser(bytes.NewReader(body))
	in, ok := handlers.ReadAndValidate[claimRequest](w, r, d.validate)
	if !ok {
		return
	}
	name := strings.ToLower(in.Name)
	if !nameRe.MatchString(name) {
		handlers.HandleError(w, dto.ErrInvalidInput("Name must be 3-32 chars of a-z 0-9 . _ -", nil))
		return
	}

	_, err = d.db.Exec(`INSERT INTO handles (name, pubkey, created_at) VALUES (?, ?, ?)`, name, pubkey, d.now().Unix())
	var sqlErr sqlite3.Error
	if errors.As(err, &sqlErr) && sqlErr.Code == sqlite3.ErrConstraint {
		handlers.HandleError(w, dto.ErrConflict("Name taken, or this key already has a handle", err))
		return
	}
	if err != nil {
		handlers.HandleError(w, err)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	_ = json.NewEncoder(w).Encode(claimResponse{name, pubkey})
}
