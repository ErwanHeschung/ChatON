// Package repository handles all direct database interactions for User data.
// It abstracts the SQL queries from the business logic layer.
package repository

import (
	"context"
	"database/sql"
	"fmt"

	"github.com/ErwanHeschung/ChatON/backend/internal/models/entity"
	"github.com/google/uuid"
)

type UserRepository struct {
	db *sql.DB
}

func NewUserRepository(db *sql.DB) *UserRepository {
	return &UserRepository{
		db: db,
	}
}

func (r *UserRepository) GetByID(ctx context.Context, ID uuid.UUID) (*entity.User, error) {
	query := `SELECT id, username, email, password FROM users WHERE id = $1 LIMIT 1`

	var user entity.User

	err := r.db.QueryRowContext(ctx, query, ID).Scan(
		&user.ID,
		&user.Username,
		&user.Email,
		&user.Password,
	)

	if err == sql.ErrNoRows {
		return nil, nil
	}

	if err != nil {
		return nil, err
	}

	return &user, nil
}

func (r *UserRepository) GetByUsername(ctx context.Context, username string) (*entity.User, error) {
	query := `SELECT id, username, email, password FROM users WHERE username = $1 LIMIT 1`

	var user entity.User

	err := r.db.QueryRowContext(ctx, query, username).Scan(
		&user.ID,
		&user.Username,
		&user.Email,
		&user.Password,
	)

	if err == sql.ErrNoRows {
		return nil, nil
	}

	if err != nil {
		return nil, err
	}

	return &user, nil
}

func (r *UserRepository) Create(ctx context.Context, user *entity.User) error {
	query := `
		INSERT INTO users (id, username, email, password)
		VALUES ($1, $2, $3, $4)
	`

	_, err := r.db.ExecContext(
		ctx, 
		query, 
		user.ID, 
		user.Username, 
		user.Email, 
		user.Password,
	)

	if err != nil {
		return err
	}

	return nil
}

func (r *UserRepository) SaveRefreshToken(ctx context.Context, rt *entity.RefreshToken) error {
    query := `
        INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at)
        VALUES ($1, $2, $3, $4)
    `
    
    _, err := r.db.ExecContext(
        ctx, 
        query, 
        rt.ID, 
        rt.UserID, 
        rt.TokenHash, 
        rt.ExpiresAt,
    )
    
    if err != nil {
        return fmt.Errorf("failed to save refresh token: %w", err)
    }
    return nil
}

func (r *UserRepository) GetRefreshTokenByHash(ctx context.Context, hash string) (*entity.RefreshToken, error) {
    var t entity.RefreshToken
    
    query := `SELECT id, user_id, token_hash, expires_at, created_at 
              FROM refresh_tokens 
              WHERE token_hash = $1`

    err := r.db.QueryRowContext(ctx, query, hash).Scan(
        &t.ID, 
        &t.UserID, 
        &t.TokenHash, 
        &t.ExpiresAt, 
        &t.CreatedAt,
    )

    if err != nil {
        return nil, err
    }

    return &t, nil
}

func (r *UserRepository) DeleteRefreshToken(ctx context.Context, hash string) error {
    query := `DELETE FROM refresh_tokens WHERE token_hash = $1`
    
    result, err := r.db.ExecContext(ctx, query, hash)
    if err != nil {
        return err
    }

    rows, _ := result.RowsAffected()
    if rows == 0 {
        return fmt.Errorf("token already used or missing")
    }

    return nil
}