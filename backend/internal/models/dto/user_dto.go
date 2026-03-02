package dto

import "github.com/google/uuid"

type UserRegisterIn struct {
	Username string `json:"username" validate:"required,min=5"`
	Email    string `json:"email" validate:"required,email"`
	Password string `json:"password" validate:"required,min=8"`
}

type UserRegisterOut struct {
	ID       uuid.UUID `json:"id"`
	Username string `json:"username"`
	Email 	 string `json:"email"`
}

type UserLoginIn struct {
	Email    string `json:"email" validate:"required,email"`
	Password string `json:"password" validate:"required,min=8"`
}

type UserLoginTokenOut struct {
	UserLoginOut     UserLoginOut
	JWTAccessToken 	 string
	RefreshToken  string
}

type UserTokenOut struct {
	JWTAccessToken 	 string
	RefreshToken  string
}

type UserLoginOut struct {
	ID       uuid.UUID `json:"id"`
	Username string `json:"username"`
	Email 	 string `json:"email"`
}