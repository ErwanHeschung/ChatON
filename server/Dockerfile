# Built on the build machine's platform, cross-compiled for the target one: pure Go (bbolt, no
# cgo), so the arm64 image for a Raspberry Pi needs no emulation.
FROM --platform=$BUILDPLATFORM golang:1.26 AS builder
ARG TARGETOS TARGETARCH

WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download
COPY . .

RUN CGO_ENABLED=0 GOOS=$TARGETOS GOARCH=$TARGETARCH go build -trimpath -ldflags="-s -w" -o chaton-server ./cmd/server
RUN mkdir /data

# CA certificates (NIP-05, NIP-11, push) and a non-root user, nothing else: no shell.
FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /app/chaton-server /chaton-server
# The whole instance lives here: the database, relay.key, vapid.key, blobs/, rules.yaml.
COPY --from=builder --chown=nonroot:nonroot /data /data

ENV SERVER_PORT=:8080 SERVER_DBPATH=/data/chaton.bolt
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/chaton-server"]
