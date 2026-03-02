// Package handlers provides the HTTP entry points for user-related actions
// such as registration, login, and profile management.
package handlers

import (
	"encoding/json"
	"net/http"
	"time"

	"github.com/ErwanHeschung/ChatON/backend/internal/models/dto"
	"github.com/ErwanHeschung/ChatON/backend/internal/service"

	"github.com/go-playground/validator/v10"
)

/*
Create instance and routes registration
*/

type UserHandler struct {
	validate *validator.Validate;
	service *service.AuthService;
}

func NewUserHandler(validator *validator.Validate, service *service.AuthService) *UserHandler {
	return &UserHandler{
        validate: validator,
		service: service,
    }
}

func (h *UserHandler) RegisterRoutes(mux *http.ServeMux) {
	mux.HandleFunc("POST /auth/register", h.RegisterUser)
	mux.HandleFunc("POST /auth/login", h.LoginUser)
	mux.HandleFunc("POST /auth/refresh", h.RefreshToken)
}

/*
Routes
*/

func (h *UserHandler) RegisterUser(w http.ResponseWriter, r *http.Request) {
	var registrationReq dto.UserRegisterIn

	registrationReq, ok := ReadAndValidate[dto.UserRegisterIn](w, r, h.validate)
	if !ok {
		return
	}

	response, err := h.service.RegisterUser(r.Context(), registrationReq)

	if err != nil {
		HandleError(w, err) 
		return
	}

	w.Header().Set("Content-Type", "application/json")
    w.WriteHeader(http.StatusCreated)
    json.NewEncoder(w).Encode(response)
}

func (h *UserHandler) LoginUser(w http.ResponseWriter, r *http.Request) {
	var loginReq dto.UserLoginIn

	loginReq, ok := ReadAndValidate[dto.UserLoginIn](w, r, h.validate)

	if !ok {
		return
	}

	response, err := h.service.LoginUser(r.Context(), loginReq)

	if err != nil {
		HandleError(w, err) 
		return
	}


	h.setAccessTokenCookie(w,response.JWTAccessToken)
	h.setRefreshTokenCookie(w,response.RefreshToken)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response.UserLoginOut)
}

func (h *UserHandler) RefreshToken(w http.ResponseWriter, r *http.Request) {
    cookie, err := r.Cookie("refresh_token")
    if err != nil {
        HandleError(w, dto.ErrUnauthorized("No refresh token provided", nil))
        return
    }

    response, err := h.service.RefreshAccessToken(r.Context(), cookie.Value)
    if err != nil {
        HandleError(w, err)
        return
    }

    h.setAccessTokenCookie(w, response.JWTAccessToken)
    h.setRefreshTokenCookie(w, response.RefreshToken)

    w.WriteHeader(http.StatusOK)
}

func (h *UserHandler) setAccessTokenCookie(w http.ResponseWriter, token string) {
    http.SetCookie(w, &http.Cookie{
        Name:     "access_token",
        Value:    token,
        Path:     "/",
        HttpOnly: true,
        Secure:   true,
        SameSite: http.SameSiteLaxMode,
        MaxAge:   15 * 60,
        Expires:  time.Now().Add(15 * time.Minute),
    })
}

func (h *UserHandler) setRefreshTokenCookie(w http.ResponseWriter, token string) {
    http.SetCookie(w, &http.Cookie{
        Name:     "refresh_token",
        Value:    token,
        Path:     "/refresh",
        HttpOnly: true,
        Secure:   true,
        SameSite: http.SameSiteStrictMode,
        MaxAge:   7 * 24 * 3600,
        Expires:  time.Now().Add(7 * 24 * time.Hour),
    })
}