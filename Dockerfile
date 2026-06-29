# Stage 1: Planner (cache recipe)
FROM rust:1.88-slim-bookworm AS planner
RUN cargo install cargo-chef --locked
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Builder (cook deps, then build app)
FROM rust:1.88-slim-bookworm AS builder
RUN apt-get update \
    && apt-get install -y pkg-config libssl-dev g++ \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked
WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --features onnx --recipe-path recipe.json

COPY . .
RUN cargo build --release --features onnx --bin tdg

# Stage 3: Runtime (distroless)
FROM gcr.io/distroless/cc-debian12 AS runtime
COPY --from=builder /app/target/release/tdg /usr/local/bin/tdg
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/tdg", "stats"]
ENTRYPOINT ["/usr/local/bin/tdg"]
CMD ["serve"]
