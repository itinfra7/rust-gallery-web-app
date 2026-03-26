#!/bin/sh

set -eu

CONFIG_FILE="${GALLERY_APP_BACKUP_CONFIG:-/usr/local/etc/gallery-app-backup.conf}"

if [ ! -r "${CONFIG_FILE}" ]; then
    echo "Missing backup config: ${CONFIG_FILE}" >&2
    exit 1
fi

. "${CONFIG_FILE}"

if [ -x /usr/local/bin/python3.11 ]; then
    PYTHON_BIN="/usr/local/bin/python3.11"
elif command -v python3 >/dev/null 2>&1; then
    PYTHON_BIN="$(command -v python3)"
elif command -v python >/dev/null 2>&1; then
    PYTHON_BIN="$(command -v python)"
else
    echo "python3 is required for restore drill orchestration." >&2
    exit 1
fi

snapshot_arg="${1:-latest}"

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
[ -f "${db_backup}" ] || { echo "Missing database backup: ${db_backup}" >&2; exit 1; }
[ -f "${uploads_backup}" ] || { echo "Missing uploads backup: ${uploads_backup}" >&2; exit 1; }
[ -r "${APP_ROOT}/.env" ] || { echo "Missing application .env at ${APP_ROOT}/.env" >&2; exit 1; }

timestamp="$(date +%Y%m%d-%H%M%S)"
db_suffix="$(date +%Y%m%d_%H%M%S)"
drill_db="gallery_app_drill_${db_suffix}"
drill_root="${BACKUP_ROOT}/drills/${timestamp}"
drill_home_root="/home/${APP_USER}/gallery-app-drills"
drill_app_root="${drill_home_root}/${timestamp}"
drill_run_root="${drill_app_root}/run"
drill_log_path="${drill_root}/drill.log"
cleanup_json="${drill_root}/cleanup.json"
summary_json="${drill_root}/summary.json"
create_db_json="${drill_root}/create-db.json"
migrate_db_json="${drill_root}/migrate-db.json"
restore_db_json="${drill_root}/restore-db.json"
http_checks_json="${drill_root}/http-checks.json"
pid_file="${drill_run_root}/drill.pid"
app_log_file="${drill_run_root}/drill.log"
tmp_db_json="$(mktemp "${TMP_DIR}/gallery-app-drill-db.XXXXXX")"

status="failed"
drill_port=18080
db_created=0
app_started=0
cleanup_db_status="not-run"
cleanup_app_status="not-run"

choose_drill_port() {
    while sockstat -4 -l 2>/dev/null | awk '{print $6}' | grep -Eq "[:.]${drill_port}\$"; do
        drill_port=$((drill_port + 1))
        if [ "${drill_port}" -gt 18120 ]; then
            echo "No free drill port found in 18080-18120." >&2
            exit 1
        fi
    done
}

write_json_file() {
    target="$1"
    tmp_target="${target}.tmp"
    cat > "${tmp_target}"
    mv -f "${tmp_target}" "${target}"
}

cleanup() {
    set +e

    if [ -f "${pid_file}" ]; then
        drill_pid="$(cat "${pid_file}" 2>/dev/null)"
        if [ -n "${drill_pid}" ]; then
            kill "${drill_pid}" 2>/dev/null || true
            sleep 1
        fi
    fi

    if [ -f "${app_log_file}" ]; then
        cp "${app_log_file}" "${drill_log_path}" 2>/dev/null || true
    fi

    if [ "${db_created}" -eq 1 ]; then
        if (cd "${APP_ROOT}" && "${DB_TOOL}" drop-db "${drill_db}") > "${drill_root}/drop-db.json" 2>"${drill_root}/drop-db.stderr"; then
            cleanup_db_status="dropped"
        else
            cleanup_db_status="drop-failed"
        fi
    fi

    if [ -d "${drill_app_root}" ]; then
        if rm -rf "${drill_app_root}"; then
            cleanup_app_status="removed"
        else
            cleanup_app_status="remove-failed"
        fi
    else
        cleanup_app_status="not-created"
    fi

    rm -f "${tmp_db_json}"

    write_json_file "${cleanup_json}" <<EOF
{
  "status": "${status}",
  "snapshot_dir": "${snapshot_dir}",
  "drill_db": "${drill_db}",
  "drill_port": ${drill_port},
  "cleanup_db_status": "${cleanup_db_status}",
  "cleanup_app_status": "${cleanup_app_status}",
  "kept_log_path": "$( [ -f "${drill_log_path}" ] && printf '%s' "${drill_log_path}" || printf 'none' )"
}
EOF
}

trap cleanup EXIT INT TERM HUP

choose_drill_port

install -d -o root -g wheel -m 750 "${BACKUP_ROOT}/drills" "${drill_root}"
install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${drill_home_root}"
install -d -o "${APP_USER}" -g "${APP_GROUP}" -m 755 "${drill_app_root}" "${drill_run_root}"

source_db_url="$(grep '^DATABASE_URL=' "${APP_ROOT}/.env" | cut -d= -f2- | sed 's/^"//; s/"$//')"
drill_db_url="$("${PYTHON_BIN}" - "${source_db_url}" "${drill_db}" <<'PY'
import sys
from urllib.parse import urlsplit, urlunsplit

url = sys.argv[1]
drill_db = sys.argv[2]
parts = urlsplit(url)
path = f"/{drill_db}"
print(urlunsplit((parts.scheme, parts.netloc, path, parts.query, parts.fragment)))
PY
)"

: > "${drill_app_root}/.env"
grep -Ev '^(DATABASE_URL|BIND_ADDR|PORT)=' "${APP_ROOT}/.env" >> "${drill_app_root}/.env" || true
cat >> "${drill_app_root}/.env" <<EOF
DATABASE_URL=${drill_db_url}
BIND_ADDR=127.0.0.1
PORT=${drill_port}
EOF
chown "${APP_USER}:${APP_GROUP}" "${drill_app_root}/.env"
chmod 640 "${drill_app_root}/.env"

ln -s "${APP_ROOT}/public" "${drill_app_root}/public"

(cd "${APP_ROOT}" && "${DB_TOOL}" create-db "${drill_db}") > "${create_db_json}"
db_created=1

(cd "${drill_app_root}" && "${DB_TOOL}" migrate-db) > "${migrate_db_json}"

/usr/bin/zstd -dqc "${db_backup}" > "${tmp_db_json}"
(cd "${drill_app_root}" && "${DB_TOOL}" restore-db "${tmp_db_json}") > "${restore_db_json}"

/usr/bin/zstd -dqc "${uploads_backup}" | tar -C "${drill_app_root}" -xf -

su -m "${APP_USER}" -c "cd '${drill_app_root}' && env BIND_ADDR=127.0.0.1 PORT='${drill_port}' /usr/sbin/daemon -p '${pid_file}' -o '${app_log_file}' <APP_ROOT>/bin/gallery-app"
app_started=1

ready=0
attempt=0
while [ "${attempt}" -lt 30 ]; do
    if fetch -qo - "http://127.0.0.1:${drill_port}/" >/dev/null 2>&1; then
        ready=1
        break
    fi
    sleep 1
    attempt=$((attempt + 1))
done

[ "${ready}" -eq 1 ] || { echo "Drill app did not become ready on port ${drill_port}." >&2; exit 1; }

"${PYTHON_BIN}" - "${drill_port}" "${http_checks_json}" <<'PY'
import json
import sys
from urllib.request import urlopen, Request

port = int(sys.argv[1])
output_path = sys.argv[2]
base = f"http://127.0.0.1:{port}"

def fetch_status(path):
    request = Request(base + path, headers={"Host": "127.0.0.1"})
    with urlopen(request) as response:
        return response.status, response.read().decode("utf-8")

home_status, _ = fetch_status("/")
api_status, api_body = fetch_status("/api/images?limit=1&page=1")
images = json.loads(api_body)
first_page_url = images[0]["page_url"] if images else ""
image_status = None
if first_page_url:
    image_status, _ = fetch_status(first_page_url)
sitemap_status, _ = fetch_status("/sitemap.xml")
rss_status, _ = fetch_status("/rss.xml")

payload = {
    "home_status": home_status,
    "api_images_status": api_status,
    "image_count_returned": len(images),
    "first_page_url": first_page_url,
    "first_image_status": image_status,
    "sitemap_status": sitemap_status,
    "rss_status": rss_status,
}

with open(output_path, "w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2)
PY

status="success"

write_json_file "${summary_json}" <<EOF
{
  "status": "${status}",
  "snapshot_dir": "${snapshot_dir}",
  "drill_root": "${drill_root}",
  "drill_db": "${drill_db}",
  "drill_port": ${drill_port},
  "create_db_json": "${create_db_json}",
  "migrate_db_json": "${migrate_db_json}",
  "restore_db_json": "${restore_db_json}",
  "http_checks_json": "${http_checks_json}"
}
EOF

echo "Restore drill completed successfully."
echo "Snapshot: ${snapshot_dir}"
echo "Drill database: ${drill_db}"
echo "Drill port: ${drill_port}"
echo "Artifacts: ${drill_root}"
