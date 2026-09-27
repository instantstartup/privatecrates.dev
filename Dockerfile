# syntax=docker/dockerfile:1
#
# PrivateCrates server image: the Rust binary plus the static website it serves on the apex host.
# Base images are pinned by digest for reproducible builds; bump tag and digest together
# (`docker buildx imagetools inspect <image:tag>` prints the digest).
#
#   docker build -t privatecrates .
#   docker run --rm -p 8080:8080 --env-file .env privatecrates

ARG RUST_IMAGE=rust:1.98-slim-trixie@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7
ARG NODE_IMAGE=node:24-trixie-slim@sha256:8ec5d7557396cfe32d21c3f9c13072355ceab22b584578ca4bb28af31120cffe
ARG RUNTIME_IMAGE=gcr.io/distroless/cc-debian13:nonroot@sha256:54df941ed0d06a1bd95ef5e0ce391fd8d9f94b64782dc9a60062727849ee3f97

# ---- Website: SvelteKit static build -> /website/build ----------------------------------------------
FROM ${NODE_IMAGE} AS website
ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0 \
    PNPM_HOME=/pnpm \
    CI=true
RUN corepack enable
WORKDIR /website
# Manifests first, so the dependency layer is reused until they change. pnpm-workspace.yaml and .npmrc
# are optional (the globs match nothing if they are absent).
COPY website/package.json website/pnpm-lock.yaml website/pnpm-workspace.yam[l] website/.npmr[c] ./
RUN pnpm install --frozen-lockfile
COPY website/ ./
RUN pnpm build

# ---- Rust: dependency caching with cargo-chef ------------------------------------------------------
FROM ${RUST_IMAGE} AS chef
RUN cargo install cargo-chef --version 0.1.78 --locked
WORKDIR /src

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS server
COPY --from=planner /src/recipe.json recipe.json
# Builds only the dependencies; this layer is cached until Cargo.toml/Cargo.lock change.
RUN cargo chef cook --release --locked -p privatecrates-server --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --locked -p privatecrates-server \
    && install -D -m 0755 target/release/privatecrates-server /out/privatecrates-server

# ---- Runtime: glibc + CA certificates, non-root, no shell ------------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime
WORKDIR /app
COPY --from=server /out/privatecrates-server /app/privatecrates-server
COPY --from=website /website/build /app/website
# PORT is overridden by Railway at run time; the server binds 0.0.0.0:$PORT.
ENV PORT=8080 \
    WEBSITE_DIR=/app/website \
    RUST_LOG=info,tower_http=info
EXPOSE 8080
# distroless ":nonroot" runs as uid/gid 65532.
USER nonroot:nonroot
ENTRYPOINT ["/app/privatecrates-server"]
