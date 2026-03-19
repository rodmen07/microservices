FROM rust:latest AS builder
WORKDIR /app
COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends sqlite3 && rm -rf /var/lib/apt/lists/* \
    && sqlite3 /tmp/build.db < reporting-service/migrations/0001_create_reports.sql \
    && DATABASE_URL=sqlite:////tmp/build.db cargo build --release -p reporting-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/reporting-service /usr/local/bin/reporting-service
ENV DATABASE_URL=sqlite:////tmp/reporting.db
ENV PORT=8080
RUN useradd -r -s /bin/false appuser
USER appuser
EXPOSE 8080
CMD ["reporting-service"]
