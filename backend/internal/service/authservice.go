package service

import (
	"context"
	"time"

	"github.com/ErwanHeschung/ChatON/backend/internal/configloader"
	"github.com/ErwanHeschung/ChatON/backend/internal/models/dto"
	"github.com/ErwanHeschung/ChatON/backend/internal/models/entity"
	"github.com/ErwanHeschung/ChatON/backend/internal/repository"
	"github.com/google/uuid"
)

/*
Create instance
*/

type AuthService struct {
	repository *repository.UserRepository
	jwtConfig *configloader.JWTConfig
}

func NewUserService(repository *repository.UserRepository, jwtConfig *configloader.JWTConfig) *AuthService {
	return &AuthService{
		repository: repository,
		jwtConfig: jwtConfig,
	}
}

/*
Business Logic
*/

func (s *AuthService) RegisterUser(ctx context.Context, req dto.UserRegisterIn) (*dto.UserRegisterOut, error) {
    if err := s.checkEmailUniqueness(ctx, req.Email); err != nil {
        return nil, err
    }

    
    hashedPassword, err := s.hashPassword(req.Password)
    if err != nil {
        return nil, err
    }

	user := &entity.User{
		ID:       uuid.New(),
		Username: req.Username,
		Email:    req.Email,
		Password: hashedPassword,
	}

	if err := s.repository.Create(ctx, user); err != nil {
		return nil, err
	}

	return &dto.UserRegisterOut{
		ID:       user.ID,
		Username: user.Username,
		Email:    user.Email,
	}, nil
}

func (s *AuthService) LoginUser(ctx context.Context, req dto.UserLoginIn) (*dto.UserLoginTokenOut, error) {
	user, err := s.getUserIfPresent(ctx, req.Email);
    if  err != nil {
        return nil, err
    }
    
	err = s.checkPassword(user.Password,req.Password)

	if err != nil {
        return nil, dto.ErrUnauthorized("Invalid email or password", nil)
    }

	jwtAccessToken, err := s.generateJWTAccess(user.ID,s.jwtConfig)
	refreshToken, hash := s.generateRefreshToken()
	s.saveRefreshTokenHash(ctx,hash,user.ID)

    if  err != nil {
        return nil, err
    }

	return &dto.UserLoginTokenOut{
		JWTAccessToken: jwtAccessToken,
		RefreshToken: refreshToken,
		UserLoginOut: dto.UserLoginOut{
			ID:       user.ID,
			Username: user.Username,
			Email:    user.Email,
		},
	}, nil
}

func (s *AuthService) RefreshAccessToken(ctx context.Context, cookieValue string) (*dto.UserTokenOut, error) {
    incomingHash := s.hashToken(cookieValue)

    storedToken, err := s.repository.GetRefreshTokenByHash(ctx, incomingHash)
    if err != nil {
        return nil, dto.ErrUnauthorized("Invalid session", nil)
    }

    if time.Now().After(storedToken.ExpiresAt) {
        s.repository.DeleteRefreshToken(ctx, incomingHash)
        return nil, dto.ErrUnauthorized("Session expired", nil)
    }

    err = s.repository.DeleteRefreshToken(ctx, incomingHash)
    if err != nil {
        return nil, dto.ErrUnauthorized("Session already rotated", nil)
    }
	
	tokenPair,err := s.GenerateTokenPair(ctx, storedToken.UserID)
	if err != nil {
        return nil, err
    }
	
	return tokenPair, nil
}

/*
Utils
*/


func (s *AuthService) checkEmailUniqueness(ctx context.Context, email string) error {
    user, err := s.repository.GetByEmail(ctx, email)
    if err != nil {
        return err
    }
    if user != nil {
        return dto.ErrConflict("Email already registered", nil)
    }
    return nil
}

func (s *AuthService) getUserIfPresent(ctx context.Context, email string) (*entity.User, error) {
    user, err := s.repository.GetByEmail(ctx, email)
    if err != nil {
        return nil,err
    }

    if user == nil {
        return nil, dto.ErrUnauthorized("Invalid email or password", nil)
    }

    return user, err
}

func (s *AuthService) saveRefreshTokenHash(ctx context.Context, hash string, userID uuid.UUID){
	refreshDuration := time.Duration(s.jwtConfig.RefreshExp) * time.Minute

	expiresAt := time.Now().Add(refreshDuration)

	refreshToken := &entity.RefreshToken{
		ID:        uuid.New(),
		UserID:    userID,
		TokenHash: hash,
		ExpiresAt: expiresAt,
	}

	s.repository.SaveRefreshToken(ctx, refreshToken)
}

func (s *AuthService) GenerateTokenPair(ctx context.Context, userID uuid.UUID) (*dto.UserTokenOut, error) {
    jwtAccessToken, err := s.generateJWTAccess(userID,s.jwtConfig)
	if err != nil {
		return nil, err
	}
	refreshToken, hash := s.generateRefreshToken()
	s.saveRefreshTokenHash(ctx,hash,userID)

	return &dto.UserTokenOut{
		JWTAccessToken: jwtAccessToken,
		RefreshToken: refreshToken,
	}, nil
}