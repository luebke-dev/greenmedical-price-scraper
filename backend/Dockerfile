# syntax=docker/dockerfile:1.7
# Build: cargo-chef caches the dependency build; SQLX_OFFLINE uses the committed .sqlx/ data.
FROM docker.io/lukemathwalker/cargo-chef:latest-rust-1.98-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
# profile.release sets strip = true, so no separate strip step is needed.
RUN cargo build --release --locked --bin greenmedical-backend

# Runtime: distroless (glibc + libgcc only, no shell), non-root UID 65532. TLS is rustls, no OpenSSL.
FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=builder /app/target/release/greenmedical-backend /usr/local/bin/greenmedical-backend
USER nonroot
EXPOSE 8080 9090
ENV HTTP_BIND=0.0.0.0:8080 \
    METRICS_BIND=0.0.0.0:9090 \
    LOG_FORMAT=json
ENTRYPOINT ["/usr/local/bin/greenmedical-backend"]
CMD ["serve"]
