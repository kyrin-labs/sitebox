# ── build ────────────────────────────────────────────────────────────────
# Alpine builds against musl, so the resulting binary is fully static and can
# be copied into the runtime image without any shared libraries.
FROM rust:1-alpine AS build

RUN apk add --no-cache musl-dev build-base
WORKDIR /src/engine
COPY engine/ ./
# The skills are embedded by build.rs; without them the binary would ship
# without the doctrine Sols is supposed to follow.
COPY skills/ /src/skills/
RUN cargo build --release

# ── runtime ──────────────────────────────────────────────────────────────
# Node is still required for sites that declare `serve = "node"` (their own
# server.js owns an HTTP API). Every other site is served in-process.
FROM node:22-alpine

RUN apk add --no-cache ca-certificates wget
WORKDIR /app

COPY --from=build /src/engine/target/release/sitebox /usr/local/bin/sitebox
COPY --from=build /src/engine/target/release/siteboxd /usr/local/bin/siteboxd
COPY --from=build /src/engine/target/release/sitebox-mcp /usr/local/bin/sitebox-mcp

COPY dashboard/ ./dashboard/
COPY sites/ ./sites/

EXPOSE 4445

HEALTHCHECK --interval=30s --timeout=5s --retries=3 --start-period=5s \
  CMD wget -qO- http://127.0.0.1:4445/api/sites >/dev/null || exit 1

CMD ["siteboxd", "--root", "/app", "--port", "4445", "--ui", "/app/dashboard/public"]
