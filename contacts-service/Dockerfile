FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends libsqlite3-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release -p contacts-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/contacts-service /usr/local/bin/contacts-service
RUN useradd -r -s /bin/false appuser && mkdir -p /data && chown appuser /data
USER appuser
EXPOSE 8080
CMD ["contacts-service"]
