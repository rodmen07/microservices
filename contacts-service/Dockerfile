FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p contacts-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/contacts-service /usr/local/bin/contacts-service
RUN useradd -r -s /bin/false appuser
USER appuser
EXPOSE 8080
CMD ["contacts-service"]
