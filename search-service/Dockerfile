FROM rust:1.82-slim AS builder

RUN apt-get update && apt-get install -y sqlite3 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# Create a local SQLite DB from migrations so sqlx macros can validate queries
RUN cat migrations/*.sql | sqlite3 /tmp/build.db &&     DATABASE_URL="sqlite:////tmp/build.db" cargo build --release --bin search-service

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/search-service /usr/local/bin/search-service

EXPOSE 8080
CMD ["/usr/local/bin/search-service"]
