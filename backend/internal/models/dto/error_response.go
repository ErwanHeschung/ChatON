package dto

import "net/http"

type AppError struct {
    Code    string            `json:"code"`
    Message string            `json:"message"`
    Details map[string]string `json:"details,omitempty"`
    Status  int               `json:"-"`
    RawErr  error             `json:"-"`
}

func (e *AppError) Error() string {
    return e.Message
}

func (e *AppError) Unwrap() error {
    return e.RawErr
}

func ErrNotFound(msg string, err error) *AppError {
    return &AppError{Code: "NOT_FOUND", Message: msg, Status: http.StatusNotFound, RawErr: err}
}

func ErrInvalidInput(msg string, err error) *AppError {
    return &AppError{Code: "INVALID_INPUT", Message: msg, Status: http.StatusBadRequest, RawErr: err}
}

func ErrConflict(msg string, err error) *AppError {
    return &AppError{Code: "CONFLICT", Message: msg, Status: http.StatusConflict, RawErr: err}
}

func ErrUnauthorized(msg string, err error) *AppError {
    return &AppError{Code: "Unauthorized", Message: msg, Status: http.StatusUnauthorized, RawErr: err}
}