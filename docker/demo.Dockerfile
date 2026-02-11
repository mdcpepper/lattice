FROM rust:1-bookworm

WORKDIR /app

RUN rustup target add wasm32-unknown-unknown \
    && cargo install trunk --locked

COPY . .

WORKDIR /app/crates/demo

EXPOSE 8080

CMD ["trunk", "serve", "--address", "0.0.0.0", "--port", "8080"]
