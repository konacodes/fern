FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false -m -d /data fern
USER fern

COPY --from=builder /build/target/release/fern /usr/local/bin/fern

ENV RUST_LOG=info

CMD ["fern"]
