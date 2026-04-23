FROM rust:1.88-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
# Create dummy main + build.rs to cache deps
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs && echo "fn main() {}" > build.rs
RUN cargo build --release && rm -rf src build.rs

# Version info — declared after dep cache so changes don't bust the deps layer.
ARG GIT_SHA=unknown
ARG BUILT_AT=unknown
ENV GIT_SHA=$GIT_SHA
ENV BUILT_AT=$BUILT_AT

COPY src ./src
COPY migrations ./migrations
COPY build.rs ./build.rs
RUN touch src/main.rs src/lib.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/wisp /usr/local/bin/wisp
ENTRYPOINT ["wisp"]
