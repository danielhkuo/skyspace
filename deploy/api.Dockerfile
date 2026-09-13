# syntax=docker/dockerfile:1.7
# One image for the whole backend: `skyspace-api` (the server) and
# `skyspace` (the operator CLI, which the migrate and jobs services run).
# Build context is the repository root: `docker compose -f deploy/compose.yml`.
#
# `rust-toolchain.toml` is deliberately not copied: the base image already
# ships 1.96.0, and copying the file would make rustup download the same
# toolchain again plus the wasm target on every cold build.

FROM rust:1.96-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock clippy.toml rustfmt.toml ./
COPY crates ./crates
# Cache mounts keep the registry and the incremental target between builds,
# so a code change rebuilds only what changed.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p skyspace-api -p skyspace-cli \
    && mkdir -p /out \
    && cp target/release/skyspace-api target/release/skyspace /out/

FROM debian:bookworm-slim
# ca-certificates: Rice and SMTP over TLS (everything is rustls, no OpenSSL).
# cron: the jobs service (deploy/jobs/). curl: the compose health check.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates cron curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --user-group --home-dir /nonexistent --shell /usr/sbin/nologin skyspace \
    && mkdir -p /archive && chown skyspace:skyspace /archive
COPY --from=build /out/skyspace-api /out/skyspace /usr/local/bin/
# The hand-encoded university-wide requirements, for `skyspace import university`.
COPY data/university /srv/data/university
COPY deploy/jobs/run-job.sh /usr/local/bin/skyspace-job
COPY deploy/jobs/cron-entrypoint.sh /usr/local/bin/skyspace-cron
COPY deploy/jobs/crontab /etc/cron.d/skyspace
RUN chmod 0644 /etc/cron.d/skyspace \
    && chmod 0755 /usr/local/bin/skyspace-job /usr/local/bin/skyspace-cron
USER skyspace
EXPOSE 8080
CMD ["skyspace-api"]
