FROM rust:1.82-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/automation-service /usr/local/bin/automation-service
EXPOSE 3014
CMD ["automation-service"]
