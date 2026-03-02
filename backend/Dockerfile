FROM dhi.io/golang:1.25-debian13-dev AS builder

WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .

RUN CGO_ENABLED=0 GOOS=linux go build -ldflags="-s -w" -o chaton-backend ./cmd/api/main.go

FROM dhi.io/golang:1.25-debian13 AS runner

WORKDIR /app

COPY --from=builder /app/chaton-backend .

COPY --from=builder /app/migrations ./migrations

USER 65534:65534 

EXPOSE 8080
ENTRYPOINT ["./chaton-backend"]