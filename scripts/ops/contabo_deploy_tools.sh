#!/usr/bin/env bash
# where: local operator machine
# what: update Contabo indexer/explorer from a git ref and restart services
# why: remove repetitive rsync/manual steps and keep deploy procedure reproducible
set -euo pipefail

SSH_HOST="${SSH_HOST:-contabo-deployer}"
REPO_URL="${REPO_URL:-$(git config --get remote.origin.url)}"
REF="${REF:-$(git rev-parse HEAD)}"
REMOTE_REPO_DIR="${REMOTE_REPO_DIR:-/opt/kasane-repo}"
REMOTE_RUNTIME_DIR="${REMOTE_RUNTIME_DIR:-/opt/kasane}"
EXPLORER_INSTALL_CMD="${EXPLORER_INSTALL_CMD:-npm ci}"
EXPLORER_INSTALL_STEP=""

case "${EXPLORER_INSTALL_CMD}" in
  "npm ci" | "ci")
    EXPLORER_INSTALL_STEP="npm ci"
    ;;
  "npm install" | "install")
    EXPLORER_INSTALL_STEP="npm install"
    ;;
  *)
    echo "[contabo-deploy-tools] EXPLORER_INSTALL_CMD must be one of: 'npm ci', 'ci', 'npm install', 'install'" >&2
    exit 1
    ;;
esac

log() {
  echo "[contabo-deploy-tools] $*"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[contabo-deploy-tools] missing command: $1" >&2
    exit 1
  fi
}

if [[ -z "${REPO_URL}" ]]; then
  echo "[contabo-deploy-tools] REPO_URL is empty (set REPO_URL or configure git remote origin)" >&2
  exit 1
fi

require_cmd ssh
require_cmd git

log "host=${SSH_HOST}"
log "repo=${REPO_URL}"
log "ref=${REF}"
log "remote_repo_dir=${REMOTE_REPO_DIR}"
log "remote_runtime_dir=${REMOTE_RUNTIME_DIR}"

ssh "${SSH_HOST}" \
  "REPO_URL='${REPO_URL}' REF='${REF}' REMOTE_REPO_DIR='${REMOTE_REPO_DIR}' REMOTE_RUNTIME_DIR='${REMOTE_RUNTIME_DIR}' EXPLORER_INSTALL_STEP='${EXPLORER_INSTALL_STEP}' bash -s" <<'REMOTE'
set -euo pipefail

echo "[contabo-deploy-tools] remote host=$(hostname)"

if [[ ! -d "${REMOTE_REPO_DIR}/.git" ]]; then
  echo "[contabo-deploy-tools] clone ${REPO_URL} -> ${REMOTE_REPO_DIR}"
  sudo mkdir -p "$(dirname "${REMOTE_REPO_DIR}")"
  sudo git clone "${REPO_URL}" "${REMOTE_REPO_DIR}"
  sudo chown -R deployer:deployer "${REMOTE_REPO_DIR}"
fi

cd "${REMOTE_REPO_DIR}"
git fetch --tags origin
git checkout --detach "${REF}"
git reset --hard "${REF}"

echo "[contabo-deploy-tools] sync tools/indexer -> ${REMOTE_RUNTIME_DIR}/tools/indexer"
sudo rsync -a --delete \
  --exclude node_modules \
  --exclude dist \
  --exclude archive \
  --exclude .env.local \
  "${REMOTE_REPO_DIR}/tools/indexer/" \
  "${REMOTE_RUNTIME_DIR}/tools/indexer/"

echo "[contabo-deploy-tools] sync tools/explorer -> ${REMOTE_RUNTIME_DIR}/tools/explorer"
sudo rsync -a --delete \
  --exclude node_modules \
  --exclude .next \
  --exclude .env.local \
  --exclude tsconfig.tsbuildinfo \
  "${REMOTE_REPO_DIR}/tools/explorer/" \
  "${REMOTE_RUNTIME_DIR}/tools/explorer/"

sudo chown -R rpcgw:rpcgw "${REMOTE_RUNTIME_DIR}/tools/indexer" "${REMOTE_RUNTIME_DIR}/tools/explorer"

echo "[contabo-deploy-tools] build indexer"
sudo -u rpcgw bash -lc "cd '${REMOTE_RUNTIME_DIR}/tools/indexer' && npm ci && npm run build"

echo "[contabo-deploy-tools] build explorer (${EXPLORER_INSTALL_STEP})"
sudo -u rpcgw bash -lc "cd '${REMOTE_RUNTIME_DIR}/tools/explorer' && ${EXPLORER_INSTALL_STEP} && npm run build"

echo "[contabo-deploy-tools] restart services"
sudo systemctl restart kasane-indexer.service
sudo systemctl restart kasane-explorer.service

echo "[contabo-deploy-tools] service status"
sudo systemctl status --no-pager --lines=0 kasane-indexer.service
sudo systemctl status --no-pager --lines=0 kasane-explorer.service
REMOTE

log "done"
