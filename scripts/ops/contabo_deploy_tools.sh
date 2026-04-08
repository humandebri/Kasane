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
EXPLORER_INSTALL_CMD="${EXPLORER_INSTALL_CMD:-pnpm install --frozen-lockfile}"
EXPLORER_INSTALL_ARGS=""

case "${EXPLORER_INSTALL_CMD}" in
  "pnpm install --frozen-lockfile" | "frozen")
    EXPLORER_INSTALL_ARGS="install --frozen-lockfile"
    ;;
  "pnpm install" | "install")
    EXPLORER_INSTALL_ARGS="install"
    ;;
  *)
    echo "[contabo-deploy-tools] EXPLORER_INSTALL_CMD must be one of: 'pnpm install --frozen-lockfile', 'frozen', 'pnpm install', 'install'" >&2
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
log "note=remote git access assumes deployer user has a read-only SSH deploy key when REPO_URL is ssh"

ssh "${SSH_HOST}" \
  "REPO_URL='${REPO_URL}' REF='${REF}' REMOTE_REPO_DIR='${REMOTE_REPO_DIR}' REMOTE_RUNTIME_DIR='${REMOTE_RUNTIME_DIR}' EXPLORER_INSTALL_ARGS='${EXPLORER_INSTALL_ARGS}' bash -s" <<'REMOTE'
set -euo pipefail

echo "[contabo-deploy-tools] remote host=$(hostname)"

build_explorer() {
  local runtime_dir="$1"
  local install_args="$2"
  sudo -u rpcgw bash -lc "
    set -euo pipefail
    cd '${runtime_dir}/tools/explorer'
    package_manager=\$(node -p \"require('./package.json').packageManager || ''\")
    if [[ -z \"\${package_manager}\" || \"\${package_manager}\" != pnpm@* ]]; then
      echo '[contabo-deploy-tools] tools/explorer packageManager must pin pnpm' >&2
      exit 1
    fi
    pnpm_version=\${package_manager#pnpm@}
    corepack prepare \"pnpm@\${pnpm_version}\" --activate >/dev/null
    pnpm_cjs=\"\$HOME/.cache/node/corepack/v1/pnpm/\${pnpm_version}/dist/pnpm.cjs\"
    if [[ ! -f \"\${pnpm_cjs}\" ]]; then
      echo \"[contabo-deploy-tools] missing pnpm entrypoint: \${pnpm_cjs}\" >&2
      exit 1
    fi
    node \"\${pnpm_cjs}\" ${install_args}
    node \"\${pnpm_cjs}\" run build
  "
}

if [[ ! -d "${REMOTE_REPO_DIR}/.git" ]]; then
  echo "[contabo-deploy-tools] clone ${REPO_URL} -> ${REMOTE_REPO_DIR}"
  sudo mkdir -p "$(dirname "${REMOTE_REPO_DIR}")"
  sudo chown deployer:deployer "$(dirname "${REMOTE_REPO_DIR}")"
  git clone "${REPO_URL}" "${REMOTE_REPO_DIR}"
  sudo chown -R deployer:deployer "${REMOTE_REPO_DIR}"
fi

cd "${REMOTE_REPO_DIR}"
git remote set-url origin "${REPO_URL}"
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

echo "[contabo-deploy-tools] build explorer (pnpm ${EXPLORER_INSTALL_ARGS})"
build_explorer "${REMOTE_RUNTIME_DIR}" "${EXPLORER_INSTALL_ARGS}"

echo "[contabo-deploy-tools] restart services"
sudo systemctl restart kasane-indexer.service
sudo systemctl restart kasane-explorer.service

echo "[contabo-deploy-tools] service status"
sudo systemctl status --no-pager --lines=0 kasane-indexer.service
sudo systemctl status --no-pager --lines=0 kasane-explorer.service
REMOTE

log "done"
