FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends sqlite3 && rm -rf /var/lib/apt/lists/* \
    && sqlite3 /tmp/build.db < search-service/migrations/0001_create_search_documents.sql \
    && DATABASE_URL=sqlite:////tmp/build.db cargo build --release -p search-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/search-service /usr/local/bin/search-service
ENV DATABASE_URL=sqlite:////tmp/search.db
ENV PORT=8080
RUN useradd -r -s /bin/false appuser
USER appuser
EXPOSE 8080
CMD ["search-service"]
