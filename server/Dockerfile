FROM dhi.io/golang:1.26-debian13-dev AS builder

WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .

RUN CGO_ENABLED=0 GOOS=linux go build -ldflags="-s -w" -o chaton-server ./cmd/server

FROM dhi.io/golang:1.26-debian13 AS runner

WORKDIR /app

COPY --from=builder /app/chaton-server .


USER 65534:65534 

EXPOSE 8080
ENTRYPOINT ["./chaton-server"]