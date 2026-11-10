# syntax=docker/dockerfile:1
# The web app (web/): installed from its lockfile with the pnpm version package.json pins.
FROM node:24-bookworm-slim AS web
WORKDIR /src/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN --mount=type=cache,target=/root/.local/share/pnpm/store pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# rust-toolchain.toml (copied in by COPY . .) picks the compiler; this tag only provides rustup and the OS.
# Keep it at the toml's version so the build doesn't download a second toolchain.
FROM rust:1.99.1-slim-bookworm AS build
WORKDIR /src
COPY . .
# Migrations are embedded at compile time (sqlx::migrate!), so no DB is needed to build.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked && cp target/release/health-mcp /health-mcp

# Pinned by digest so Dependabot proposes updates.
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f
COPY --from=build /health-mcp /health-mcp
COPY --from=web /src/web/build /web
ENV WEB_DIR=/web
EXPOSE 8080
# distroless nonroot, numeric so Kubernetes runAsNonRoot can verify it.
USER 65532:65532
ENTRYPOINT ["/health-mcp"]
