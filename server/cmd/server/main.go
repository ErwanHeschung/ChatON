package main

import (
	"log"
	"net/http"

	"github.com/ErwanHeschung/ChatON/server/configs"
	"github.com/ErwanHeschung/ChatON/server/internal/configloader"
)

func main() {
	cfg, err := configloader.LoadConfig()
	if err != nil {
		log.Fatal("Cannot load config:", err)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status": "UP"}`))
	})

	log.Printf("Server starting on %s...", cfg.Server.Port)
	if err := http.ListenAndServe(cfg.Server.Port, configs.SetupCORS(cfg, mux)); err != nil {
		log.Fatal(err)
	}
}
