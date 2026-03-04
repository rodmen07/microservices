FROM rust:1.82-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/reporting-service /usr/local/bin/reporting-service
EXPOSE 3017
CMD ["reporting-service"]
