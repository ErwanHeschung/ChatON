package main

import (
	"log"
	"net/http"

	"github.com/fiatjaf/eventstore/sqlite3"

	"github.com/ErwanHeschung/ChatON/server/configs"
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
		s.DBPath = "chaton.db"
	}
	if s.PublicURL == "" {
		s.PublicURL = "http://localhost" + s.Port
	}
	if s.Name == "" {
		s.Name = "ChatON"
	}

	// WAL + busy timeout: relay and directory write to the same file.
	db := &sqlite3.SQLite3Backend{DatabaseURL: s.DBPath + "?_journal_mode=WAL&_busy_timeout=5000"}
	if err := db.Init(); err != nil {
		log.Fatal("Cannot open database:", err)
	}
	defer db.Close()

	dir, err := directory.New(db.DB, s.PublicURL)
	if err != nil {
		log.Fatal("Cannot init directory:", err)
	}
	rl := relay.New(relay.Config{
		Name:           s.Name,
		Description:    "ChatON instance",
		BlockedPubkeys: s.BlockedPubkeys,
	}, db, dir)

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
