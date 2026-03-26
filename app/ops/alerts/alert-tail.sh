#!/bin/sh

set -eu

ALERT_LIB="${GALLERY_APP_ALERT_LIB:-/usr/local/libexec/gallery-app-alert-lib.sh}"

if [ ! -r "${ALERT_LIB}" ]; then
    echo "Missing alert library: ${ALERT_LIB}" >&2
    exit 1
fi

# shellcheck disable=SC1090
. "${ALERT_LIB}"
gallery_app_alert_load_config

lines="${1:-20}"
tail -n "${lines}" "${ALERT_SPOOL_FILE}"
