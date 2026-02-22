#!/usr/bin/env bash
# Shared OpenBao helper functions sourced by justfile recipes.

# openbao_admin_db_url <pg_host>
#
# Fetches admin DB credentials from OpenBao KV and prints a postgresql:// URL
# to stdout. Returns 1 if OPENBAO_ADDR or OPENBAO_TOKEN is unset.
#
# Environment variables read:
#   OPENBAO_ADDR                 OpenBao server address
#   OPENBAO_TOKEN                OpenBao token
#   OPENBAO_POSTGRES_SECRET_PATH KV path (default: secret/data/lattice/postgres)
#   POSTGRES_DB                  Database name (default: lattice_db)
openbao_admin_db_url() {
  local pg_host="${1}"
  local openbao_addr="${OPENBAO_ADDR:-}"
  local openbao_token="${OPENBAO_TOKEN:-}"
  local openbao_postgres_secret_path="${OPENBAO_POSTGRES_SECRET_PATH:-secret/data/lattice/postgres}"
  local postgres_db="${POSTGRES_DB:-lattice_db}"

  if [[ -z "${openbao_addr}" || -z "${openbao_token}" ]]; then
    return 1
  fi

  local secret_json
  if ! secret_json="$(curl --silent --fail \
    --header "X-Vault-Token: ${openbao_token}" \
    "${openbao_addr}/v1/${openbao_postgres_secret_path}" 2>/dev/null)"; then
    return 1
  fi

  local admin_user admin_password
  admin_user="$(printf '%s' "${secret_json}" | jq -r '.data.data.admin_user // empty')"
  admin_password="$(printf '%s' "${secret_json}" | jq -r '.data.data.admin_password // empty')"

  if [[ -z "${admin_user}" || -z "${admin_password}" ]]; then
    printf 'error: admin_user or admin_password missing from OpenBao secret\n' >&2
    return 1
  fi

  local encoded_user encoded_password
  encoded_user="$(printf '%s' "${admin_user}" | jq -Rr @uri)"
  encoded_password="$(printf '%s' "${admin_password}" | jq -Rr @uri)"

  printf 'postgresql://%s:%s@%s:5432/%s\n' \
    "${encoded_user}" "${encoded_password}" "${pg_host}" "${postgres_db}"
}
