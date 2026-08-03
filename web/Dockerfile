FROM rust:1.95-bookworm AS builder

WORKDIR /app

RUN cargo install topcoat-cli --version 0.5.0 --locked

COPY Cargo.toml Cargo.lock ./
COPY checker ./checker
COPY cli ./cli
COPY db ./db
COPY web ./web

RUN topcoat asset bundle \
    --release \
    --package aur_ai_security_web \
    --out /app/dist/assets


FROM debian:bookworm-slim AS runtime

RUN groupadd --system app \
    && useradd --system --gid app --home-dir /app app \
    && mkdir -p /app/assets /data \
    && chown -R app:app /app /data

WORKDIR /app

COPY --from=builder --chown=app:app /app/target/release/aur_ai_security_web ./aur_ai_security_web
COPY --from=builder --chown=app:app /app/dist/assets ./assets

ENV HOST=0.0.0.0 \
    PORT=3000 \
    AUR_AI_SECURITY_DATABASE=/data/sqlite.db

VOLUME ["/data"]
EXPOSE 3000

USER app

ENTRYPOINT ["./aur_ai_security_web"]
