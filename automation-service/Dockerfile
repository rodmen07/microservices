FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p automation-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/automation-service /usr/local/bin/automation-service
RUN useradd -r -s /bin/false appuser
USER appuser
EXPOSE 8080
CMD ["automation-service"]
