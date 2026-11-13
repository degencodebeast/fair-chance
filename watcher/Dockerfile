# Build stage. go.mod: go 1.26, toolchain go1.26.2, no dependencies.
FROM golang:1.26-alpine AS build
WORKDIR /src
COPY go.mod ./
COPY cmd ./cmd
COPY internal ./internal
# timetzdata embeds the time zone data, because the run image has none and TZ alone would fall back to UTC.
RUN CGO_ENABLED=0 go build -trimpath -tags timetzdata -ldflags="-s -w" -o /out/watcher ./cmd/watcher

# Run stage: no shell, non-root, no port. Set FC_SENDER=telegram to start the Telegram bot.
FROM gcr.io/distroless/static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab
COPY --from=build /out/watcher /watcher
ENTRYPOINT ["/watcher"]
