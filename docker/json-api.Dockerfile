FROM rust:1-bookworm

WORKDIR /app

COPY . .

RUN cargo build --release -p lattice-json

ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8698
ENV RUST_LOG=info

EXPOSE 8698

CMD ["./target/release/lattice-json"]
