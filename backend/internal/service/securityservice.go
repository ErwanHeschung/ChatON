package service

import (
	"crypto/sha256"
	"encoding/hex"
	"time"

	"github.com/ErwanHeschung/ChatON/backend/internal/configloader"
	"github.com/golang-jwt/jwt/v5"
	"github.com/google/uuid"
	"golang.org/x/crypto/bcrypt"
)

func (s *AuthService) hashPassword(password string) (string, error) {
	hashedBytes, err := bcrypt.GenerateFromPassword([]byte(password), bcrypt.DefaultCost)
	if err != nil {
		return "", err
	}
	return string(hashedBytes), nil
}

func (s *AuthService) checkPassword(hashedPassword, plainPassword string) error {
    return bcrypt.CompareHashAndPassword([]byte(hashedPassword), []byte(plainPassword))
}

func (s *AuthService) generateJWTAccess(userID uuid.UUID, jwtConfig *configloader.JWTConfig) (string, error) {
    claims := jwt.MapClaims{
        "sub": userID,
        "exp": time.Now().Add(time.Minute * time.Duration(jwtConfig.JWTExp)).Unix(),
        "iat": time.Now().Unix(),
    }

    token := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)

   	return token.SignedString([]byte(jwtConfig.JWTSecret))
}

func (s *AuthService) generateRefreshToken() (string, string) {
    token := uuid.New().String() 

    hash := s.hashToken(token)

    return token, hash
}

func (s *AuthService) hashToken(token string) string{
	h := sha256.Sum256([]byte(token))
    hash := hex.EncodeToString(h[:])

    return hash
}