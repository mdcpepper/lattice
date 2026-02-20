FROM rust:1-bookworm

WORKDIR /app

RUN cargo install watchexec-cli

ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8698
ENV RUST_LOG=info

EXPOSE 8698

CMD ["watchexec", "--watch", "crates/json-api/src", "--exts", "rs", "--restart", "--", "cargo", "run", "--package", "lattice-json"]
