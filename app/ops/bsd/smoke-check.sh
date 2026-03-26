#!/bin/sh

set -eu

SERVICE_NAME="${GALLERY_APP_SERVICE_NAME:-gallery_app}"
APP_ROOT="${GALLERY_APP_ROOT:-<APP_ROOT>}"
APP_USER="<APP_USER>"
APP_GROUP="<APP_GROUP>"
OPS_STATUS_DIR="${APP_ROOT}/run/ops-status"
LATEST_SMOKE_FILE="${OPS_STATUS_DIR}/latest-smoke-check.json"
SMOKE_HISTORY_FILE="${OPS_STATUS_DIR}/smoke-history.ndjson"
ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"
BASE_URL="${1:-http://127.0.0.1:8080}"
TIMEOUT_SECONDS="${GALLERY_APP_SMOKE_TIMEOUT_SECONDS:-10}"
WAIT_ATTEMPTS="${GALLERY_APP_SMOKE_WAIT_ATTEMPTS:-15}"
CURL_BIN="${GALLERY_APP_CURL_BIN:-/usr/local/bin/curl}"
PERL_BIN="${GALLERY_APP_PERL_BIN:-/usr/local/bin/perl}"

alert_enabled=0
last_step="initializing"
representative_path=""
detail="smoke check not started"

if [ -r "${ALERT_LIB}" ]; then
    # shellcheck disable=SC1090
    . "${ALERT_LIB}"
    alert_enabled=1
fi

emit_smoke_alert() {
    [ "${alert_enabled}" -eq 1 ] || return 0
    gallery_app_emit_alert "$@"
}

json_escape() {
    printf '%s' "${1:-}" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_ops_status() {
    target_file="$1"
    tmp_file="${OPS_STATUS_DIR}/.$(basename "${target_file}").tmp"
    cat > "${tmp_file}"
    chown "${APP_USER}:${APP_GROUP}" "${tmp_file}"
    chmod 644 "${tmp_file}"
    mv -f "${tmp_file}" "${target_file}"
}

append_history() {
    record="$1"
    tmp_file="${OPS_STATUS_DIR}/.smoke-history.ndjson.tmp"
    {
        if [ -f "${SMOKE_HISTORY_FILE}" ]; then
            cat "${SMOKE_HISTORY_FILE}"
        fi
        printf '%s\n' "${record}"
    } | tail -n 64 > "${tmp_file}"
    chown "${APP_USER}:${APP_GROUP}" "${tmp_file}"
    chmod 644 "${tmp_file}"
    mv -f "${tmp_file}" "${SMOKE_HISTORY_FILE}"
}

record_status() {
    result_status="$1"
    recorded_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    representative_json='null'
    if [ -n "${representative_path}" ]; then
        representative_json="\"$(json_escape "${representative_path}")\""
    fi
    checked_paths_json='["/","/api/images?limit=1&page=1","/sitemap.xml","/rss.xml"]'
    if [ -n "${representative_path}" ]; then
        checked_paths_json="[\"/\",\"/api/images?limit=1&page=1\",\"/sitemap.xml\",\"/rss.xml\",\"$(json_escape "${representative_path}")\"]"
    fi

    status_json=$(cat <<EOF
{
  "status": "${result_status}",
  "recorded_at": "${recorded_at}",
  "base_url": "$(json_escape "${BASE_URL}")",
  "checked_paths": ${checked_paths_json},
  "representative_path": ${representative_json},
  "detail": "$(json_escape "${detail}")"
}
EOF
)
    write_ops_status "${LATEST_SMOKE_FILE}" <<EOF
${status_json}
EOF
    append_history "$(printf '%s' "${status_json}" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g; s/^ //; s/ $//')"
}

finish() {
    exit_status=$?
    if [ "${exit_status}" -ne 0 ]; then
        record_status "failed" || true
        emit_smoke_alert \
            "error" \
            "smoke-check" \
            "smoke_check_failed" \
            "gallery-app smoke check failed" \
            "base_url=${BASE_URL}; step=${last_step}; detail=${detail}" \
            "smoke-check-failed:${BASE_URL}" \
            "yes" || true
    fi
    exit "${exit_status}"
}

trap finish EXIT INT TERM HUP

install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${OPS_STATUS_DIR}"

wait_for_service() {
    attempts=0
    while [ "${attempts}" -lt "${WAIT_ATTEMPTS}" ]; do
        if "${CURL_BIN}" -fsS --max-time "${TIMEOUT_SECONDS}" -o /dev/null "${BASE_URL}/"; then
            return 0
        fi
        attempts=$((attempts + 1))
        sleep 1
    done
    detail="service did not answer ${BASE_URL}/ within ${WAIT_ATTEMPTS} attempts"
    return 1
}

fetch_path() {
    path="$1"
    target_file="$2"
    "${CURL_BIN}" -fsS --max-time "${TIMEOUT_SECONDS}" -o "${target_file}" "${BASE_URL}${path}"
}

last_step="wait-for-service"
wait_for_service

tmp_api_json="$(mktemp)"
tmp_home_html="$(mktemp)"
tmp_sitemap_xml="$(mktemp)"
tmp_rss_xml="$(mktemp)"
tmp_image_html="$(mktemp)"

cleanup_tmp() {
    rm -f "${tmp_api_json}" "${tmp_home_html}" "${tmp_sitemap_xml}" "${tmp_rss_xml}" "${tmp_image_html}"
}
trap 'cleanup_tmp; finish' EXIT INT TERM HUP

last_step="home"
fetch_path "/" "${tmp_home_html}"

last_step="api-images"
fetch_path "/api/images?limit=1&page=1" "${tmp_api_json}"

last_step="sitemap"
fetch_path "/sitemap.xml" "${tmp_sitemap_xml}"

last_step="rss"
fetch_path "/rss.xml" "${tmp_rss_xml}"

last_step="representative-image-path"
representative_path="$("${PERL_BIN}" -MJSON::PP -0777 -e '
    my $payload = do { local $/; <> };
    my $json = JSON::PP->new->utf8;
    my $data = $json->decode($payload);
    if (ref($data) eq "ARRAY" && @$data) {
        print $data->[0]{page_url} // "";
    }
' "${tmp_api_json}")"

if [ -z "${representative_path}" ]; then
    detail="api returned no representative page_url"
    exit 1
fi

last_step="representative-image-page"
fetch_path "${representative_path}" "${tmp_image_html}"

detail="home, api, sitemap, rss, and ${representative_path} all returned success"
record_status "success"

emit_smoke_alert \
    "info" \
    "smoke-check" \
    "smoke_check_success" \
    "gallery-app smoke check completed" \
    "base_url=${BASE_URL}; representative_path=${representative_path}" \
    "smoke-check-success:${BASE_URL}" \
    "no" || true

cleanup_tmp
trap - EXIT INT TERM HUP
