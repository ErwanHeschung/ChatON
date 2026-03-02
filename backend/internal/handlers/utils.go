package handlers

import (
	"encoding/json"
	"net/http"
	"errors"
	"log"
	"strings"
	
	"github.com/go-playground/validator/v10"

	"github.com/ErwanHeschung/ChatON/backend/internal/models/dto"
)

func ReadAndValidate[T any](w http.ResponseWriter, r *http.Request, v *validator.Validate) (T, bool) {
	var input T

	if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
		appErr := dto.ErrInvalidInput("Malformed JSON request body", err)
		HandleError(w, appErr) 
		return input, false
	}

	if err := v.Struct(input); err != nil {
		appErr := dto.ErrInvalidInput("Validation failed", err)
		HandleError(w, appErr)
		return input, false
	}

	return input, true
}

func HandleError(w http.ResponseWriter, err error) {
	var appErr *dto.AppError

	if errors.As(err, &appErr) {
        appErr.Details = formatValidationErrors(appErr.RawErr)

        w.Header().Set("Content-Type", "application/json")
        w.WriteHeader(appErr.Status)
        json.NewEncoder(w).Encode(appErr)
        return
    }

	log.Printf("[CRITICAL] Unexpected error: %v", err)
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusInternalServerError)
	json.NewEncoder(w).Encode(map[string]string{"message": "Internal server error"})
}

func formatValidationErrors(err error) map[string]string {
	errorsMap := make(map[string]string)
	
	var vErrors validator.ValidationErrors
	if errors.As(err, &vErrors) {
		for _, f := range vErrors {
			name := f.Field()
			if len(name) > 0 {
				name = strings.ToLower(name[:1]) + name[1:]
			}
			errorsMap[name] = "failed validation: " + f.Tag()
		}
	}
	return errorsMap
}