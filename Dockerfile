# syntax=docker/dockerfile:1
#
# Backend image: the Rust API only. The UI is built and hosted separately, so
# nothing under `web/` is copied in (that also keeps the build context small and
# the Rust layer cache stable when only frontend files change).
#
# Pair it with a UI built from `web/` using:
#     VITE_API_BASE=https://api.example.com npm run build
# and serve that `dist/` from Vercel, Cloudflare Pages, or any static host. Set
# ALLOWED_ORIGIN on this container to the UI's origin so the browser is allowed
# to call it.

FROM rust:1-alpine AS build

RUN apk add --no-cache musl-dev
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/bin && \
    echo 'fn main() {}' > src/main.rs && \
    echo 'pub fn stub() {}' > src/lib.rs && \
    echo 'fn main() {}' > src/bin/serve.rs && \
    cargo build --release --bin serve

COPY src ./src
RUN touch src/lib.rs src/bin/serve.rs && cargo build --release --bin serve

FROM alpine:3.20

RUN apk add --no-cache ca-certificates && \
    adduser -D -H -u 10001 app

COPY --from=build /app/target/release/serve /usr/local/bin/serve

USER app
ENV PORT=8080
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
    CMD wget -qO- "http://127.0.0.1:${PORT}/healthz" >/dev/null || exit 1

ENTRYPOINT ["/usr/local/bin/serve"]
