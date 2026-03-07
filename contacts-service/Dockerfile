FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p contacts-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/contacts-service /usr/local/bin/contacts-service
EXPOSE 8080
CMD ["sh", "-c", "mkdir -p /data && contacts-service"]
