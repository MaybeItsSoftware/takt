# The sync server, for Cloud Run (scripts/deploy_sync_server.sh) or
# docker-compose. Build context is this directory, so nothing outside it is
# copied.

FROM rust:1-slim AS builder
WORKDIR /build

# Dependencies first, against stub sources, so a source-only change reuses
# this layer instead of recompiling sqlx, axum and tokio.
COPY Cargo.toml Cargo.lock ./
# The rules the server shares with the clients, a path dependency in this
# directory, so it is part of the build context.
COPY rules ./rules
RUN mkdir src \
 && echo 'fn main() {}' > src/main.rs \
 && touch src/lib.rs \
 && cargo build --release --locked \
 && rm -rf src

# `sqlx::migrate!` embeds the migrations at compile time, so they are build
# input, not something the runtime image needs to carry.
COPY migrations ./migrations
COPY static ./static
COPY src ./src
# Make the real sources newer than the stub build's fingerprints.
RUN touch src/main.rs src/lib.rs \
 && cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 priority
COPY --from=builder /build/target/release/takt-sync-server /usr/local/bin/takt-sync-server
USER priority
ENV PORT=8080
EXPOSE 8080
# Exec form, so the server is PID 1 and receives Cloud Run's SIGTERM itself; it
# handles it with a graceful shutdown that answers open long-polls first.
CMD ["/usr/local/bin/takt-sync-server"]
