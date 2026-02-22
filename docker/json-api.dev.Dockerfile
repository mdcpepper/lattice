FROM rust:1-bookworm

WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates jq \
    && rm -rf /var/lib/apt/lists/* \
    && cargo install watchexec-cli \
    && cargo install sqlx-cli \
    && chmod -R a+rwX /usr/local/cargo \
    && mkdir -p /app/target \
    && chmod -R a+rwX /app/target

ENV PATH="/usr/local/cargo/bin:${PATH}"
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8698
ENV RUST_LOG=info

EXPOSE 8698

CMD ["bash", "/app/docker/json-api.dev-entrypoint.sh"]
