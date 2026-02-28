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
    docker compose --profile dev up --build --force-recreate json-api-dev demo grafana

remove:
    docker compose --profile dev down --volumes --remove-orphans --rmi local

clear-logs:
    #!/usr/bin/env bash
    set -euo pipefail
    services=(alloy tempo loki pyroscope prometheus postgres-exporter grafana)
    volumes=(lattice_alloy_data lattice_tempo_data lattice_loki_data lattice_pyroscope_data lattice_prometheus_data lattice_grafana_data)

    docker compose --profile dev rm -f -s "${services[@]}" >/dev/null 2>&1 || true

    to_remove=()
    for volume in "${volumes[@]}"; do
      if docker volume inspect "$volume" >/dev/null 2>&1; then
        to_remove+=("$volume")
      fi
    done

    if [ "${#to_remove[@]}" -gt 0 ]; then
      docker volume rm "${to_remove[@]}" >/dev/null
      echo "Cleared observability data: ${to_remove[*]}"
    else
      echo "No observability data volumes found."
    fi

reset-observability: clear-logs
    docker compose --profile dev up -d --wait tempo loki pyroscope alloy prometheus grafana
    echo "Observability stack restarted with fresh data."

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
