package main

import (
	"log"
	"net/http"
	"path/filepath"

	"fiatjaf.com/nostr"
	"fiatjaf.com/nostr/eventstore/boltdb"

	"github.com/ErwanHeschung/ChatON/server/configs"
	"github.com/ErwanHeschung/ChatON/server/internal/community"
	"github.com/ErwanHeschung/ChatON/server/internal/configloader"
	"github.com/ErwanHeschung/ChatON/server/internal/directory"
	"github.com/ErwanHeschung/ChatON/server/internal/relay"
)

func main() {
	cfg, err := configloader.LoadConfig()
	if err != nil {
		log.Fatal("Cannot load config:", err)
	}
	s := &cfg.Server
	if s.DBPath == "" {
		s.DBPath = "chaton.bolt"
	}
	if s.PublicURL == "" {
		s.PublicURL = "http://localhost" + s.Port
	}
	if s.Name == "" {
		s.Name = "ChatON"
	}

	// One bbolt file: events (eventstore) and handles (directory buckets).
	db := &boltdb.BoltBackend{Path: s.DBPath}
	if err := db.Init(); err != nil {
		log.Fatal("Cannot open database:", err)
	}
	defer db.Close()

	dir, err := directory.New(db.DB, s.PublicURL)
	if err != nil {
		log.Fatal("Cannot init directory:", err)
	}
	blocked := make([]nostr.PubKey, 0, len(s.BlockedPubkeys))
	for _, hex := range s.BlockedPubkeys {
		pk, err := nostr.PubKeyFromHex(hex)
		if err != nil {
			log.Fatalf("SERVER_BLOCKEDPUBKEYS: %q: %v", hex, err)
		}
		blocked = append(blocked, pk)
	}
	key, err := community.LoadKey(s.RelayKey, filepath.Join(filepath.Dir(s.DBPath), "relay.key"))
	if err != nil {
		log.Fatal("Cannot load relay key:", err)
	}
	groups, err := community.New(key, dir.RelayURL(), db, dir)
	if err != nil {
		log.Fatal("Cannot load communities:", err)
	}
	rl := relay.New(relay.Config{
		Name:           s.Name,
		Description:    "ChatON instance",
		BlockedPubkeys: blocked,
	}, db, dir, groups)

	mux := rl.Router() // khatru serves WebSocket + NIP-11 on "/", the rest falls through here
	dir.Register(mux)
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status": "UP"}`))
	})

	log.Printf("ChatON %s on %s (db %s)", s.PublicURL, s.Port, s.DBPath)
	if err := http.ListenAndServe(s.Port, configs.SetupCORS(cfg, rl)); err != nil {
		log.Fatal(err)
	}
}
