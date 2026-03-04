FROM rust:1.82-slim AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/contacts-service /usr/local/bin/contacts-service
EXPOSE 3011
CMD ["contacts-service"]
