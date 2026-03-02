package main

import (
	"database/sql"
	"fmt"
	"log"
	"net/http"

	"github.com/ErwanHeschung/ChatON/backend/internal/configloader"
	"github.com/ErwanHeschung/ChatON/backend/internal/handlers"
	"github.com/ErwanHeschung/ChatON/backend/internal/repository"
	"github.com/ErwanHeschung/ChatON/backend/internal/service"

	"github.com/go-playground/validator/v10"

	"github.com/golang-migrate/migrate/v4"
	"github.com/golang-migrate/migrate/v4/database/postgres"
	_ "github.com/golang-migrate/migrate/v4/source/file"
	_ "github.com/lib/pq"
)

func main() {
	cfg, err := configloader.LoadConfig()

	if err != nil {
		log.Fatal("Cannot load config:", err)
	}

	db := InitDB(cfg)

	validator := validator.New()
	
	userRepository := repository.NewUserRepository(db);
	userService := service.NewUserService(userRepository, &cfg.JWTConfig);
	userHandler := handlers.NewUserHandler(validator, userService)

	mux := http.NewServeMux()
	userHandler.RegisterRoutes(mux)

	log.Printf("Server starting on %s...", cfg.Server.Port)

	addHealthCheck(mux)

	if err := http.ListenAndServe(cfg.Server.Port, mux); err != nil {
		log.Fatal(err)
	}
}

func addHealthCheck(mux *http.ServeMux) {
    mux.HandleFunc("GET /user/health", func(w http.ResponseWriter, r *http.Request) {
        w.Header().Set("Content-Type", "application/json")
        w.WriteHeader(http.StatusOK)
        w.Write([]byte(`{"status": "UP"}`))
    })
}

func InitDB(cfg *configloader.Config) *sql.DB {
	dbSource := fmt.Sprintf(
		"host=%s port=%d user=%s password=%s dbname=%s sslmode=disable",
		cfg.Database.Host,
		cfg.Database.Port,
		cfg.Database.User,
		cfg.Database.Password,
		cfg.Database.DBName,
	)

	db, err := sql.Open("postgres", dbSource)
	if err != nil {
		log.Fatalf("Could not open DB connection: %v", err)
	}

	if err := db.Ping(); err != nil {
		log.Fatalf("Could not ping DB: %v", err)
	}

	log.Println("Applying database migrations...")
	if err := runDatabaseMigrations(db); err != nil {
		log.Fatalf("Migration failed: %v", err)
	}

	log.Println("Database initialized successfully.")
	return db
}

func runDatabaseMigrations(db *sql.DB) error {
    driver, err := postgres.WithInstance(db, &postgres.Config{})
    if err != nil {
        return err
    }

    m, err := migrate.NewWithDatabaseInstance(
        "file://migrations", 
        "postgres",
        driver,
    )

    if err != nil {
        return err
    }

    if err := m.Up(); err != nil && err != migrate.ErrNoChange {
        return err
    }

    return nil
}