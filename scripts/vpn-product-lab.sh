#!/usr/bin/env bash
set -euo pipefail

readonly DEFAULT_NVPN_REV="4c43cc5761d67e5dc1a9a4de30c829ae45dc37f3"
readonly DEFAULT_NVPN_GIT_URL="https://github.com/mmalmi/nostr-vpn.git"

nvpn_rev="${IRIS_STACK_NVPN_REV:-$DEFAULT_NVPN_REV}"
nvpn_git_url="${IRIS_STACK_NVPN_GIT_URL:-$DEFAULT_NVPN_GIT_URL}"
dry_run="${IRIS_STACK_NVPN_DRY_RUN:-false}"

if [[ ! "$nvpn_rev" =~ ^[0-9a-f]{40}$ ]]; then
  echo "vpn product lab: IRIS_STACK_NVPN_REV must be an exact 40-character lowercase commit" >&2
  exit 2
fi

case "$dry_run" in
  false | 0) ;;
  true | 1) ;;
  *)
    echo "vpn product lab: IRIS_STACK_NVPN_DRY_RUN must be true, false, 1, or 0" >&2
    exit 2
    ;;
esac

command -v git >/dev/null 2>&1 || {
  echo "vpn product lab: git is required" >&2
  exit 2
}

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/iris-stack-nvpn.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT
repo_dir="$tmp_dir/nostr-vpn"

git init --quiet "$repo_dir"
git -C "$repo_dir" remote add origin "$nvpn_git_url"
git -C "$repo_dir" fetch --quiet --depth 1 origin "$nvpn_rev"
git -C "$repo_dir" checkout --quiet --detach FETCH_HEAD

actual_rev="$(git -C "$repo_dir" rev-parse HEAD)"
if [[ "$actual_rev" != "$nvpn_rev" ]]; then
  echo "vpn product lab: fetched $actual_rev instead of requested $nvpn_rev" >&2
  exit 1
fi

canonical_gate="$repo_dir/scripts/e2e-connect-docker.sh"
if [[ ! -x "$canonical_gate" ]]; then
  echo "vpn product lab: pinned artifact does not contain executable scripts/e2e-connect-docker.sh" >&2
  exit 1
fi

echo "vpn product lab: verified Nostr VPN commit $actual_rev"
if [[ "$dry_run" == "true" || "$dry_run" == "1" ]]; then
  exit 0
fi

command -v docker >/dev/null 2>&1 || {
  echo "vpn product lab: docker is required" >&2
  exit 2
}
docker compose version >/dev/null 2>&1 || {
  echo "vpn product lab: the Docker Compose plugin is required" >&2
  exit 2
}
command -v jq >/dev/null 2>&1 || {
  echo "vpn product lab: jq is required" >&2
  exit 2
}

export NVPN_E2E_PROJECT_NAME="${NVPN_E2E_PROJECT_NAME:-iris-stack-nvpn-product-$$}"
(
  cd "$repo_dir"
  "$canonical_gate"
)
