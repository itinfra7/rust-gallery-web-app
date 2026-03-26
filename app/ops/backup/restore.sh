#!/bin/sh

set -eu

CONFIG_FILE="${GALLERY_APP_BACKUP_CONFIG:-/usr/local/etc/gallery-app-backup.conf}"
DRY_RUN=0
DB_ONLY=0
UPLOADS_ONLY=0
CONFIRMED=0

usage() {
    echo "usage: gallery-app-restore.sh [--dry-run] [--db-only|--uploads-only] [--yes] <snapshot-dir|latest>" >&2
    exit 1
}

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            ;;
        --db-only)
            DB_ONLY=1
            ;;
        --uploads-only)
            UPLOADS_ONLY=1
            ;;
        --yes)
            CONFIRMED=1
            ;;
        --help|-h)
            usage
            ;;
        *)
            break
            ;;
    esac
    shift
done

[ $# -eq 1 ] || usage

if [ "${DB_ONLY}" -eq 1 ] && [ "${UPLOADS_ONLY}" -eq 1 ]; then
    echo "--db-only and --uploads-only cannot be used together" >&2
    exit 1
fi

if [ ! -r "${CONFIG_FILE}" ]; then
    echo "Missing backup config: ${CONFIG_FILE}" >&2
    exit 1
fi

. "${CONFIG_FILE}"

snapshot_arg="$1"

case "${snapshot_arg}" in
    latest)
        snapshot_dir="${BACKUP_ROOT}/latest"
        ;;
    *)
        snapshot_dir="${snapshot_arg}"
        ;;
esac

db_backup="${snapshot_dir}/db.json.zst"
uploads_backup="${snapshot_dir}/uploads.tar.zst"

[ -d "${snapshot_dir}" ] || { echo "Missing snapshot directory: ${snapshot_dir}" >&2; exit 1; }
[ "${UPLOADS_ONLY}" -eq 1 ] || [ -f "${db_backup}" ] || { echo "Missing database backup: ${db_backup}" >&2; exit 1; }
[ "${DB_ONLY}" -eq 1 ] || [ -f "${uploads_backup}" ] || { echo "Missing uploads backup: ${uploads_backup}" >&2; exit 1; }

tmp_db_json=""
cleanup() {
    [ -n "${tmp_db_json}" ] && rm -f "${tmp_db_json}"
}
trap cleanup EXIT INT TERM HUP

if [ "${DRY_RUN}" -eq 1 ]; then
    echo "Restore dry-run for ${snapshot_dir}"

    if [ "${UPLOADS_ONLY}" -eq 0 ]; then
        tmp_db_json="$(mktemp "${TMP_DIR}/gallery-app-restore-check.XXXXXX")"
        zstd -dqc "${db_backup}" > "${tmp_db_json}"
        (cd "${APP_ROOT}" && "${DB_TOOL}" validate-db-backup "${tmp_db_json}")
    fi

    if [ "${DB_ONLY}" -eq 0 ]; then
        /usr/bin/zstd -dqc "${uploads_backup}" | tar -tf - >/dev/null
    fi

    echo "Dry-run validation completed successfully."
    exit 0
fi

if [ "${CONFIRMED}" -ne 1 ]; then
    echo "Actual restore requires --yes." >&2
    exit 1
fi

timestamp="$(date +%Y%m%d-%H%M%S)"
safeguard_dir="${PRE_RESTORE_DIR}/${timestamp}"

install -d -o root -g wheel -m 750 "${BACKUP_ROOT}" "${PRE_RESTORE_DIR}" "${safeguard_dir}"

echo "Creating pre-restore safeguard in ${safeguard_dir}"

service_was_running=0
if service gallery_app onestatus >/dev/null 2>&1; then
    service_was_running=1
    service gallery_app stop
fi

if [ "${UPLOADS_ONLY}" -eq 0 ]; then
    tmp_db_json="$(mktemp "${TMP_DIR}/gallery-app-pre-restore.XXXXXX")"
    (cd "${APP_ROOT}" && "${DB_TOOL}" backup-db "${tmp_db_json}") > "${safeguard_dir}/db-summary.json"
    zstd -T0 -19 -q -o "${safeguard_dir}/db.json.zst" "${tmp_db_json}"
    rm -f "${tmp_db_json}"
    tmp_db_json="$(mktemp "${TMP_DIR}/gallery-app-restore.XXXXXX")"
    zstd -dqc "${db_backup}" > "${tmp_db_json}"
fi

if [ "${DB_ONLY}" -eq 0 ]; then
    mv "${UPLOADS_DIR}" "${safeguard_dir}/uploads"
fi

if [ "${UPLOADS_ONLY}" -eq 0 ]; then
    (cd "${APP_ROOT}" && "${DB_TOOL}" restore-db "${tmp_db_json}")
fi

if [ "${DB_ONLY}" -eq 0 ]; then
    /usr/bin/zstd -dqc "${uploads_backup}" | tar -C "${APP_ROOT}" -xf -
fi

if [ "${service_was_running}" -eq 1 ]; then
    service gallery_app start
fi

echo "Restore completed successfully from ${snapshot_dir}"
echo "Pre-restore safeguard is stored at ${safeguard_dir}"
