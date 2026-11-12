# Celluloplast | Parcours salarié — image unique (API + worker + interface web).

# 1. Interface web
FROM node:22-bookworm-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

# 2. Serveur Rust
FROM rust:1-trixie AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release --locked -p server

# 3. Exécution : Debian minimale, utilisateur non-root, police Unicode pour les PDF
FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates fonts-dejavu-core \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /app --shell /usr/sbin/nologin celluloplast \
 && mkdir -p /var/lib/celluloplast \
 && chown celluloplast /var/lib/celluloplast
COPY --from=server /src/target/release/celluloplast /usr/local/bin/celluloplast
COPY --from=web /web/dist /app/web
ENV WEB_DIR=/app/web \
    DATA_DIR=/var/lib/celluloplast \
    BIND_ADDR=0.0.0.0:8080 \
    LOG_FORMAT=json \
    TZ=Africa/Algiers
USER celluloplast
VOLUME ["/var/lib/celluloplast"]
EXPOSE 8080
CMD ["celluloplast", "serve"]
