#!/bin/sh

set -eu

: "${GALLERY_APP_ALERT_CONFIG:=/usr/local/etc/gallery-app-alert.conf}"

gallery_app_alert_load_config() {
    if [ -r "${GALLERY_APP_ALERT_CONFIG}" ]; then
        # shellcheck disable=SC1090
        . "${GALLERY_APP_ALERT_CONFIG}"
    fi

    : "${ALERT_APP_ROOT:=<APP_ROOT>}"
    : "${ALERT_APP_USER:=<APP_USER>}"
    : "${ALERT_APP_GROUP:=<APP_GROUP>}"
    : "${ALERT_OPS_STATUS_DIR:=${ALERT_APP_ROOT}/run/ops-status}"
    : "${ALERT_SPOOL_FILE:=${ALERT_APP_ROOT}/run/alerts.ndjson}"
    : "${ALERT_DEDUPE_DIR:=${ALERT_OPS_STATUS_DIR}/alert-dedupe}"
    : "${ALERT_STREAK_DIR:=${ALERT_OPS_STATUS_DIR}/alert-streaks}"
    : "${ALERT_NOTIFY_INFO:=no}"
    : "${ALERT_DEDUPE_COOLDOWN_SECONDS:=900}"
    : "${ALERT_FAILURE_STREAK_THRESHOLD:=1}"
    : "${ALERT_TELEGRAM_TIMEOUT_SECONDS:=15}"
    : "${ALERT_TELEGRAM_BOT_TOKEN:=}"
    : "${ALERT_TELEGRAM_CHAT_ID:=}"
    : "${ALERT_CURL_BIN:=/usr/local/bin/curl}"

    if [ ! -x "${ALERT_CURL_BIN}" ]; then
        ALERT_CURL_BIN=$(command -v curl 2>/dev/null || printf 'curl')
    fi
}

gallery_app_alert_compact() {
    printf '%s' "${1:-}" | tr '\n' ' ' | sed 's/[[:space:]][[:space:]]*/ /g; s/^ //; s/ $//'
}

gallery_app_alert_json_escape() {
    gallery_app_alert_compact "${1:-}" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

gallery_app_alert_safe_hostname() {
    hostname -s 2>/dev/null || hostname
}

gallery_app_alert_init_storage() {
    install -d -o "${ALERT_APP_USER}" -g "${ALERT_APP_GROUP}" -m 755 "${ALERT_OPS_STATUS_DIR}"
    install -d -o "${ALERT_APP_USER}" -g "${ALERT_APP_GROUP}" -m 755 "${ALERT_DEDUPE_DIR}"
    install -d -o "${ALERT_APP_USER}" -g "${ALERT_APP_GROUP}" -m 755 "${ALERT_STREAK_DIR}"

    spool_dir=$(dirname "${ALERT_SPOOL_FILE}")
    install -d -o "${ALERT_APP_USER}" -g "${ALERT_APP_GROUP}" -m 755 "${spool_dir}"

    if [ ! -f "${ALERT_SPOOL_FILE}" ]; then
        : > "${ALERT_SPOOL_FILE}"
        chown "${ALERT_APP_USER}:${ALERT_APP_GROUP}" "${ALERT_SPOOL_FILE}"
        chmod 640 "${ALERT_SPOOL_FILE}"
    fi
}

gallery_app_alert_kind_key() {
    printf '%s' "${1:-}" | tr '[:lower:]-' '[:upper:]_' | tr -cd 'A-Z0-9_'
}

gallery_app_alert_failure_threshold() {
    severity="$1"
    kind="$2"
    override_key=''
    override_name=''
    override_value=''

    case "${severity}" in
        critical|error|warn|warning)
            ;;
        *)
            printf '1\n'
            return 0
            ;;
    esac

    override_key=$(gallery_app_alert_kind_key "${kind}")
    if [ -n "${override_key}" ]; then
        override_name="ALERT_KIND_FAILURE_STREAK_THRESHOLD_${override_key}"
        eval "override_value=\${${override_name}:-}"
        case "${override_value}" in
            ''|*[!0-9]*)
                ;;
            *)
                if [ "${override_value}" -gt 0 ]; then
                    printf '%s\n' "${override_value}"
                    return 0
                fi
                ;;
        esac
    fi

    case "${ALERT_FAILURE_STREAK_THRESHOLD}" in
        ''|*[!0-9]*)
            printf '1\n'
            ;;
        *)
            if [ "${ALERT_FAILURE_STREAK_THRESHOLD}" -gt 0 ]; then
                printf '%s\n' "${ALERT_FAILURE_STREAK_THRESHOLD}"
            else
                printf '1\n'
            fi
            ;;
    esac
}

gallery_app_alert_reset_failure_streak() {
    kind="$1"
    failure_key=''
    streak_file=''

    case "${kind}" in
        *_success)
            failure_key=$(gallery_app_alert_kind_key "${kind%_success}_failed")
            ;;
        *)
            return 0
            ;;
    esac

    [ -n "${failure_key}" ] || return 0
    streak_file="${ALERT_STREAK_DIR}/${failure_key}.state"
    rm -f "${streak_file}"
}

gallery_app_alert_next_failure_streak() {
    severity="$1"
    kind="$2"
    force_send="${3:-no}"
    threshold=1
    current_count=0
    next_count=0
    kind_key=''
    streak_file=''

    if [ "${force_send}" = "yes" ]; then
        printf '1 1\n'
        return 0
    fi

    threshold=$(gallery_app_alert_failure_threshold "${severity}" "${kind}")
    kind_key=$(gallery_app_alert_kind_key "${kind}")

    case "${severity}" in
        critical|error|warn|warning)
            ;;
        *)
            printf '1 1\n'
            return 0
            ;;
    esac

    [ -n "${kind_key}" ] || {
        printf '1 %s\n' "${threshold}"
        return 0
    }

    streak_file="${ALERT_STREAK_DIR}/${kind_key}.state"
    if [ -f "${streak_file}" ]; then
        current_count=$(cat "${streak_file}" 2>/dev/null || printf '0')
        case "${current_count}" in
            ''|*[!0-9]*)
                current_count=0
                ;;
        esac
    fi

    next_count=$((current_count + 1))
    printf '%s\n' "${next_count}" > "${streak_file}"
    chown "${ALERT_APP_USER}:${ALERT_APP_GROUP}" "${streak_file}"
    chmod 640 "${streak_file}"
    printf '%s %s\n' "${next_count}" "${threshold}"
}

gallery_app_alert_write_latest_status() {
    target_file="${ALERT_OPS_STATUS_DIR}/latest-alert-delivery.json"
    tmp_file="${ALERT_OPS_STATUS_DIR}/.latest-alert-delivery.json.tmp"
    cat > "${tmp_file}"
    chown "${ALERT_APP_USER}:${ALERT_APP_GROUP}" "${tmp_file}"
    chmod 644 "${tmp_file}"
    mv -f "${tmp_file}" "${target_file}"
}

gallery_app_alert_append_record() {
    record_json="$1"
    tmp_file="${ALERT_OPS_STATUS_DIR}/.alerts.append.tmp"
    printf '%s\n' "${record_json}" > "${tmp_file}"
    chown "${ALERT_APP_USER}:${ALERT_APP_GROUP}" "${tmp_file}"
    chmod 640 "${tmp_file}"
    cat "${tmp_file}" >> "${ALERT_SPOOL_FILE}"
    rm -f "${tmp_file}"
}

gallery_app_alert_should_send() {
    severity="$1"
    kind="$2"
    force_send="${3:-no}"
    override_name=''
    override_key=''
    override_value=''

    if [ "${force_send}" = "yes" ]; then
        return 0
    fi

    override_key=$(printf '%s' "${kind}" | tr '[:lower:]-' '[:upper:]_' | tr -cd 'A-Z0-9_')
    if [ -n "${override_key}" ]; then
        override_name="ALERT_KIND_NOTIFY_${override_key}"
        eval "override_value=\${${override_name}:-}"
        case "${override_value}" in
            yes)
                return 0
                ;;
            no)
                return 1
                ;;
        esac
    fi

    case "${severity}" in
        critical|error|warn|warning)
            return 0
            ;;
        info)
            [ "${ALERT_NOTIFY_INFO}" = "yes" ]
            ;;
        *)
            return 1
            ;;
    esac
}

gallery_app_alert_is_suppressed() {
    dedupe_key="$1"
    force_send="${2:-no}"
    gallery_app_alert_suppressed_duplicates=0
    dedupe_id=''
    stamp_file=''
    now_epoch=0
    last_epoch=0
    suppressed_count=0
    stamp_payload=''

    if [ "${force_send}" = "yes" ] || [ -z "${dedupe_key}" ]; then
        return 1
    fi

    dedupe_id=$(printf '%s' "${dedupe_key}" | cksum | awk '{print $1}')
    stamp_file="${ALERT_DEDUPE_DIR}/${dedupe_id}"
    now_epoch=$(date +%s)

    if [ -f "${stamp_file}" ]; then
        stamp_payload=$(cat "${stamp_file}" 2>/dev/null || printf '')
        set -- ${stamp_payload}
        last_epoch="${1:-0}"
        suppressed_count="${2:-0}"
        case "${last_epoch}" in
            ''|*[!0-9]*)
                last_epoch=0
                ;;
        esac
        case "${suppressed_count}" in
            ''|*[!0-9]*)
                suppressed_count=0
                ;;
        esac
        if [ $((now_epoch - last_epoch)) -lt "${ALERT_DEDUPE_COOLDOWN_SECONDS}" ]; then
            suppressed_count=$((suppressed_count + 1))
            printf '%s %s\n' "${last_epoch}" "${suppressed_count}" > "${stamp_file}"
            chown "${ALERT_APP_USER}:${ALERT_APP_GROUP}" "${stamp_file}"
            chmod 640 "${stamp_file}"
            gallery_app_alert_suppressed_duplicates="${suppressed_count}"
            return 0
        fi
    fi

    printf '%s %s\n' "${now_epoch}" "0" > "${stamp_file}"
    chown "${ALERT_APP_USER}:${ALERT_APP_GROUP}" "${stamp_file}"
    chmod 640 "${stamp_file}"
    gallery_app_alert_suppressed_duplicates="${suppressed_count}"
    return 1
}

gallery_app_alert_send_telegram() {
    message_text="$1"

    if [ -z "${ALERT_TELEGRAM_BOT_TOKEN}" ] || [ -z "${ALERT_TELEGRAM_CHAT_ID}" ]; then
        return 3
    fi

    "${ALERT_CURL_BIN}" -fsS --max-time "${ALERT_TELEGRAM_TIMEOUT_SECONDS}" \
        -X POST "https://api.telegram.org/bot${ALERT_TELEGRAM_BOT_TOKEN}/sendMessage" \
        --data-urlencode "chat_id=${ALERT_TELEGRAM_CHAT_ID}" \
        --data-urlencode "text=${message_text}" \
        >/dev/null
}

gallery_app_emit_alert() {
    severity="$1"
    source="$2"
    kind="$3"
    summary="$4"
    detail="$5"
    dedupe_key="${6:-}"
    force_send="${7:-no}"

    gallery_app_alert_load_config
    gallery_app_alert_init_storage

    recorded_at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    host_name=$(gallery_app_alert_safe_hostname)
    summary_compact=$(gallery_app_alert_compact "${summary}")
    detail_compact=$(gallery_app_alert_compact "${detail}")
    failure_streak=1
    failure_threshold=1
    streak_gate_open=yes
    gallery_app_alert_suppressed_duplicates=0
    delivery_status="recorded-only"
    augmented_detail="${detail_compact}"

    gallery_app_alert_reset_failure_streak "${kind}"
    set -- $(gallery_app_alert_next_failure_streak "${severity}" "${kind}" "${force_send}")
    failure_streak="${1:-1}"
    failure_threshold="${2:-1}"
    if [ "${failure_streak}" -lt "${failure_threshold}" ]; then
        streak_gate_open=no
    fi

    if [ "${streak_gate_open}" != "yes" ]; then
        delivery_status="suppressed"
    elif gallery_app_alert_is_suppressed "${dedupe_key}" "${force_send}"; then
        delivery_status="suppressed"
    elif gallery_app_alert_should_send "${severity}" "${kind}" "${force_send}"; then
        if [ "${gallery_app_alert_suppressed_duplicates}" -gt 0 ]; then
            augmented_detail="${detail_compact}; suppressed_duplicates=${gallery_app_alert_suppressed_duplicates}"
        fi
        telegram_text=$(printf '[%s] %s\nhost=%s\nsource=%s\nkind=%s\n%s' \
            "${severity}" \
            "${summary_compact}" \
            "${host_name}" \
            "${source}" \
            "${kind}" \
            "${augmented_detail}")

        if gallery_app_alert_send_telegram "${telegram_text}"; then
            delivery_status="sent"
        else
            send_status=$?
            if [ "${send_status}" -eq 3 ]; then
                delivery_status="skipped-config"
            else
                delivery_status="failed"
            fi
        fi
    fi

    record_json=$(cat <<EOF
{"timestamp":"$(gallery_app_alert_json_escape "${recorded_at}")","host":"$(gallery_app_alert_json_escape "${host_name}")","source":"$(gallery_app_alert_json_escape "${source}")","severity":"$(gallery_app_alert_json_escape "${severity}")","kind":"$(gallery_app_alert_json_escape "${kind}")","summary":"$(gallery_app_alert_json_escape "${summary_compact}")","detail":"$(gallery_app_alert_json_escape "${augmented_detail}")","dedupe_key":"$(gallery_app_alert_json_escape "${dedupe_key}")","delivery_status":"$(gallery_app_alert_json_escape "${delivery_status}")"}
EOF
)
    record_json=$(printf '%s' "${record_json}" | sed "s/}\$/,\"failure_streak\":${failure_streak},\"failure_threshold\":${failure_threshold},\"suppressed_duplicates\":${gallery_app_alert_suppressed_duplicates}}/")

    gallery_app_alert_append_record "${record_json}"

    gallery_app_alert_write_latest_status <<EOF
{
  "recorded_at": "${recorded_at}",
  "host": "${host_name}",
  "source": "$(gallery_app_alert_json_escape "${source}")",
  "severity": "$(gallery_app_alert_json_escape "${severity}")",
  "kind": "$(gallery_app_alert_json_escape "${kind}")",
  "summary": "$(gallery_app_alert_json_escape "${summary_compact}")",
  "detail": "$(gallery_app_alert_json_escape "${augmented_detail}")",
  "dedupe_key": "$(gallery_app_alert_json_escape "${dedupe_key}")",
  "delivery_status": "$(gallery_app_alert_json_escape "${delivery_status}")",
  "failure_streak": ${failure_streak},
  "failure_threshold": ${failure_threshold},
  "suppressed_duplicates": ${gallery_app_alert_suppressed_duplicates}
}
EOF
}
