FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends sqlite3 && rm -rf /var/lib/apt/lists/* \
    && sqlite3 /tmp/build.db < opportunities-service/migrations/0001_create_opportunities.sql \
    && DATABASE_URL=sqlite:////tmp/build.db cargo build --release -p opportunities-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/opportunities-service /usr/local/bin/opportunities-service
EXPOSE 8080
CMD ["opportunities-service"]
