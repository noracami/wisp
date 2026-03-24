FROM rust:1.88-slim AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# Create dummy main to cache deps
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs
RUN cargo build --release && rm -rf src

COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs src/lib.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/wisp /usr/local/bin/wisp
ENTRYPOINT ["wisp"]
