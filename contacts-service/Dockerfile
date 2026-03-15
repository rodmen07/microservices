FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p contacts-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/contacts-service /usr/local/bin/contacts-service
RUN useradd -r -s /bin/false appuser && mkdir -p /data && chown appuser /data
USER appuser
EXPOSE 8080
CMD ["sh", "-c", "touch /data/contacts.db && contacts-service"]
