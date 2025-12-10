# Build stage
FROM rust:1.84 as builder

WORKDIR /app

COPY . .

RUN cargo build --release

# Production stage
FROM debian:bookworm-slim

WORKDIR /usr/local/bin

# Install libpq (Postgres client library) as it's required by the 'postgres' crate at runtime
RUN apt-get update && apt-get install -y libpq-dev && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rust-docker-pg-crud- .

CMD ["./rust-docker-pg-crud-"]