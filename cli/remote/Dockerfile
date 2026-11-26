# takt-mcp, the hosted MCP server (cli/src/remote, docs/remote-mcp.md).
#
# Built by scripts/deploy_mcp_server.sh from a staging directory holding
# only what the build reads: cli/, core/ and sync-server/rules/ (the core's
# clock and merge), laid out as in the repository so the path dependencies
# resolve. No target/ directories travel.
FROM rust:1-slim AS build
WORKDIR /src
COPY sync-server/rules sync-server/rules
COPY core core
COPY cli cli
RUN cargo build --release --locked --manifest-path cli/Cargo.toml --features remote --bin takt-mcp

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/cli/target/release/takt-mcp /usr/local/bin/takt-mcp
# The workspaces live here, in the instance's memory-backed filesystem.
RUN useradd --uid 10001 --create-home priority && mkdir -p /data && chown priority /data
USER priority
ENV PORT=8080 DATA_DIR=/data
CMD ["/usr/local/bin/takt-mcp"]
