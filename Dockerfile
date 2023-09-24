FROM rust:1.72-slim-bookworm as builder

WORKDIR /root
COPY src/ ./src/
COPY Cargo.toml .
COPY Cargo.lock .

RUN cargo fetch

RUN cargo build --release &&  \
    cargo test --release
