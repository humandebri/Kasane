#!/usr/bin/env bash
# where: local operator machine
# what: update Contabo rpc-gateway from a git ref and restart service
# why: keep gateway deploy reproducible and consistent with git-based ops flow
set -euo pipefail

SSH_HOST="${SSH_HOST:-contabo-deployer}"
REPO_URL="${REPO_URL:-$(git config --get remote.origin.url)}"
REF="${REF:-$(git rev-parse HEAD)}"
REMOTE_REPO_DIR="${REMOTE_REPO_DIR:-/opt/kasane-repo}"
REMOTE_RUNTIME_DIR="${REMOTE_RUNTIME_DIR:-/opt/kasane}"
SERVICE_NAME="${SERVICE_NAME:-rpc-gateway.service}"
INSTALL_CMD="${INSTALL_CMD:-npm ci}"
INSTALL_STEP=""

case "${INSTALL_CMD}" in
  "npm ci" | "ci")
    INSTALL_STEP="npm ci"
    ;;
  "npm install" | "install")
    INSTALL_STEP="npm install"
    ;;
  *)
    echo "[contabo-deploy-gateway] INSTALL_CMD must be one of: 'npm ci', 'ci', 'npm install', 'install'" >&2
    exit 1
    ;;
esac

log() {
  echo "[contabo-deploy-gateway] $*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[contabo-deploy-gateway] missing command: $1" >&2
    exit 1
  fi
}

if [[ -z "${REPO_URL}" ]]; then
  echo "[contabo-deploy-gateway] REPO_URL is empty (set REPO_URL or configure git remote origin)" >&2
  exit 1
fi

require_cmd ssh
require_cmd git

log "host=${SSH_HOST}"
log "repo=${REPO_URL}"
log "ref=${REF}"
log "remote_repo_dir=${REMOTE_REPO_DIR}"
log "remote_runtime_dir=${REMOTE_RUNTIME_DIR}"
log "service=${SERVICE_NAME}"

ssh "${SSH_HOST}" \
  "REPO_URL='${REPO_URL}' REF='${REF}' REMOTE_REPO_DIR='${REMOTE_REPO_DIR}' REMOTE_RUNTIME_DIR='${REMOTE_RUNTIME_DIR}' SERVICE_NAME='${SERVICE_NAME}' INSTALL_STEP='${INSTALL_STEP}' bash -s" <<'REMOTE'
set -euo pipefail

echo "[contabo-deploy-gateway] remote host=$(hostname)"

if [[ ! -d "${REMOTE_REPO_DIR}/.git" ]]; then
  echo "[contabo-deploy-gateway] clone ${REPO_URL} -> ${REMOTE_REPO_DIR}"
  sudo mkdir -p "$(dirname "${REMOTE_REPO_DIR}")"
  sudo git clone "${REPO_URL}" "${REMOTE_REPO_DIR}"
  sudo chown -R deployer:deployer "${REMOTE_REPO_DIR}"
fi

cd "${REMOTE_REPO_DIR}"
git fetch --tags origin
git checkout --detach "${REF}"
git reset --hard "${REF}"

echo "[contabo-deploy-gateway] sync tools/rpc-gateway -> ${REMOTE_RUNTIME_DIR}/tools/rpc-gateway"
sudo rsync -a --delete \
  --exclude node_modules \
  --exclude dist \
  --exclude .env.local \
  "${REMOTE_REPO_DIR}/tools/rpc-gateway/" \
  "${REMOTE_RUNTIME_DIR}/tools/rpc-gateway/"

sudo chown -R rpcgw:rpcgw "${REMOTE_RUNTIME_DIR}/tools/rpc-gateway"

echo "[contabo-deploy-gateway] build rpc-gateway (${INSTALL_STEP})"
sudo -u rpcgw bash -lc "cd '${REMOTE_RUNTIME_DIR}/tools/rpc-gateway' && ${INSTALL_STEP} && npm run build"

echo "[contabo-deploy-gateway] restart service ${SERVICE_NAME}"
sudo systemctl restart "${SERVICE_NAME}"

echo "[contabo-deploy-gateway] service status"
sudo systemctl status --no-pager --lines=0 "${SERVICE_NAME}"
REMOTE

log "done"
