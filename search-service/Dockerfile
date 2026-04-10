FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p search-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/search-service /usr/local/bin/search-service
RUN useradd -r -s /bin/false appuser
USER appuser
EXPOSE 8080
CMD ["search-service"]
