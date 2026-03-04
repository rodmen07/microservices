FROM rust:1.82-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/activities-service /usr/local/bin/activities-service
EXPOSE 3013
CMD ["activities-service"]
