FROM dhi.io/golang:1.26-debian13-dev AS builder

WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .

# cgo: the SQLite driver (mattn/go-sqlite3) is C. The runner is glibc too.
RUN CGO_ENABLED=1 GOOS=linux go build -ldflags="-s -w" -o chaton-server ./cmd/server
RUN mkdir /data

FROM dhi.io/golang:1.26-debian13 AS runner

WORKDIR /app
COPY --from=builder /app/chaton-server .
COPY --from=builder --chown=65534:65534 /data /data

ENV SERVER_PORT=:8080 SERVER_DBPATH=/data/chaton.db
VOLUME /data
USER 65534:65534

EXPOSE 8080
ENTRYPOINT ["./chaton-server"]
