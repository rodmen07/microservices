FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p activities-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/activities-service /usr/local/bin/activities-service
EXPOSE 8080
CMD ["sh", "-c", "mkdir -p /data && activities-service"]
