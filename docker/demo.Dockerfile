FROM rust:1-bookworm

WORKDIR /app

ARG RUST_TOOLCHAIN=1.93.0

RUN rustup toolchain install ${RUST_TOOLCHAIN} \
    && rustup default ${RUST_TOOLCHAIN} \
    && rustup target add wasm32-unknown-unknown --toolchain ${RUST_TOOLCHAIN} \
    && cargo +${RUST_TOOLCHAIN} install trunk --locked

COPY . .

WORKDIR /app/crates/demo

EXPOSE 8080

CMD ["trunk", "serve", "--address", "0.0.0.0", "--port", "8080"]
