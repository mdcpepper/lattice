set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

ext_so := "target/debug/liblattice.so"

default: test

cloc:
    cloc fixtures crates php --exclude_dir vendor,dist

test-rust:
    cargo test --all-features --workspace --exclude lattice-php-ext

build-extension:
    cd crates/php-ext && cargo build

test-extension: build-extension
    cd php && php -d extension=../{{ ext_so }} vendor/bin/pest --configuration=phpunit.xml

test: test-rust test-extension

dev:
    docker compose up -d --wait postgres
    docker compose --profile dev up --build --force-recreate json-api-dev demo

remove:
    docker compose --profile dev down --volumes --remove-orphans --rmi local

sqlx *args='':
    #!/usr/bin/env bash
    set -euo pipefail
    export OPENBAO_ADDR="${OPENBAO_ADDR:-http://localhost:8200}"
    export OPENBAO_TOKEN="${OPENBAO_TOKEN:-${OPENBAO_DEV_ROOT_TOKEN:-lattice-dev-root-token}}"
    source docker/openbao-helpers.sh
    if ! db_url="$(openbao_admin_db_url "postgres")"; then
      db_url="postgresql://${POSTGRES_USER:-lattice_user}:${POSTGRES_PASSWORD:-lattice_password}@postgres:5432/${POSTGRES_DB:-lattice_db}"
    fi
    docker compose --profile dev run --rm --build --quiet-build -T -e DATABASE_URL="$db_url" json-api-dev sqlx {{ args }}

migrate:
    just sqlx migrate run

cli *args='':
    #!/usr/bin/env bash
    set -euo pipefail
    export OPENBAO_ADDR="${OPENBAO_ADDR:-http://localhost:8200}"
    export OPENBAO_TOKEN="${OPENBAO_TOKEN:-${OPENBAO_DEV_ROOT_TOKEN:-lattice-dev-root-token}}"
    source docker/openbao-helpers.sh
    if ! db_url="$(openbao_admin_db_url "localhost")"; then
      db_url="${DATABASE_ADMIN_URL:-postgresql://${POSTGRES_USER:-lattice_user}:${POSTGRES_PASSWORD:-lattice_password}@localhost:5432/${POSTGRES_DB:-lattice_db}}"
    fi
    DATABASE_URL="$db_url" cargo run --package lattice-app -- {{ args }}

db-admin-creds:
    #!/usr/bin/env bash
    set -euo pipefail
    export OPENBAO_ADDR="${OPENBAO_ADDR:-http://localhost:8200}"
    export OPENBAO_TOKEN="${OPENBAO_TOKEN:-${OPENBAO_DEV_ROOT_TOKEN:-lattice-dev-root-token}}"
    source docker/openbao-helpers.sh
    if ! db_url="$(openbao_admin_db_url "localhost")"; then
      echo "error: could not fetch admin DB credentials from OpenBao at ${OPENBAO_ADDR}" >&2
      exit 1
    fi
    userinfo="${db_url#postgresql://}"; userinfo="${userinfo%%@*}"
    echo "user:     ${userinfo%%:*}"
    echo "password: ${userinfo#*:}"
    echo "url:      ${db_url}"
