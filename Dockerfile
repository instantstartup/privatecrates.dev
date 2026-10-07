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
# The server is its own Cargo workspace (crates/privatecrates-server), depending by path on crates beside it, which
# inherit settings from the root Cargo.toml: all of them are copied.
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates ./crates
RUN cd crates/privatecrates-server && cargo chef prepare --recipe-path /src/recipe.json

FROM chef AS server
# A read-only GitHub token for the worldbuilding-dev PrivateCrates registry, where privatecrates-qos is published:
# a sealed Railway variable, which Railway passes to builds as a build argument. Declared in this stage only, so the
# image that runs has no trace of it; Cargo uses it as a plain token for that registry.
ARG PRIVATECRATES_TOKEN
ENV CARGO_REGISTRIES_WORLDBUILDING_DEV_CREDENTIAL_PROVIDER=cargo:token
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY crates/privatecrates-server/Cargo.toml crates/privatecrates-server/Cargo.lock crates/privatecrates-server/
# Path dependencies outside the server's workspace, which cargo-chef does not stub: small, so copied whole.
COPY crates/privatecrates-common crates/privatecrates-common
COPY crates/privatecrates-verify crates/privatecrates-verify
COPY crates/privatecrates-testkit crates/privatecrates-testkit
COPY --from=planner /src/recipe.json recipe.json
# Builds only the dependencies; this layer is cached until the server's Cargo.toml/Cargo.lock change.
RUN cd crates/privatecrates-server \
    && CARGO_REGISTRIES_WORLDBUILDING_DEV_TOKEN="$PRIVATECRATES_TOKEN" \
       cargo chef cook --release --locked --recipe-path /src/recipe.json
COPY crates ./crates
RUN cd crates/privatecrates-server \
    && CARGO_REGISTRIES_WORLDBUILDING_DEV_TOKEN="$PRIVATECRATES_TOKEN" cargo build --release --locked \
    && install -D -m 0755 target/release/privatecrates-server /out/privatecrates-server

# ---- Runtime: glibc + CA certificates, non-root, no shell ------------------------------------------
FROM ${RUNTIME_IMAGE} AS runtime
WORKDIR /app
COPY --from=server /out/privatecrates-server /app/privatecrates-server
COPY --from=website /website/build /app/website
# PORT is overridden by Railway at run time; the server binds 0.0.0.0:$PORT.
ENV PORT=8080 \
    WEBSITE_DIR=/app/website \
    RUST_LOG=info
EXPOSE 8080
# distroless ":nonroot" runs as uid/gid 65532.
USER nonroot:nonroot
ENTRYPOINT ["/app/privatecrates-server"]
