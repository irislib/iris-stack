#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
install_root=${IRIS_STACK_PRODUCT_INSTALL_ROOT:-"$repo_dir/target/product-lab"}
drive_rev=${IRIS_STACK_DRIVE_REV:-05751a828f2a20b3ed46d09569e6cf35ac4c537d}
mkdir -p "$install_root"

if [ -n "${IRIS_STACK_HTREE_BIN:-}" ]; then
  test -x "$IRIS_STACK_HTREE_BIN" || {
    echo "IRIS_STACK_HTREE_BIN is not executable: $IRIS_STACK_HTREE_BIN" >&2
    exit 1
  }
else
  htree_version=${IRIS_STACK_HTREE_VERSION:-0.2.146}
  cargo install \
    --locked \
    --root "$install_root" \
    --version "=$htree_version" \
    --features fips-webrtc,git-remote-wrapper \
    --bin htree \
    --bin git-remote-htree \
    hashtree-cli
  IRIS_STACK_HTREE_BIN=$install_root/bin/htree
fi

if [ -n "${IRIS_STACK_DRIVE_FIXTURE_BIN:-}" ]; then
  test -x "$IRIS_STACK_DRIVE_FIXTURE_BIN" || {
    echo "IRIS_STACK_DRIVE_FIXTURE_BIN is not executable: $IRIS_STACK_DRIVE_FIXTURE_BIN" >&2
    exit 1
  }
else
  drive_git=${IRIS_STACK_DRIVE_GIT:-htree://npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-drive}
  case "$drive_git" in
    htree://*)
      if [ ! -x "$install_root/bin/git-remote-htree" ]; then
        cargo install --locked --root "$install_root" --version =0.2.89 git-remote-htree
      fi
      PATH="$install_root/bin:$PATH"
      export PATH
      ;;
  esac
  CARGO_NET_GIT_FETCH_WITH_CLI=true cargo install \
    --locked \
    --root "$install_root" \
    --git "$drive_git" \
    --rev "$drive_rev" \
    --features stack-fixture \
    --bin iris-drive-stack-fixture \
    iris-drive-core
  IRIS_STACK_DRIVE_FIXTURE_BIN=$install_root/bin/iris-drive-stack-fixture
fi

if [ -n "${IRIS_STACK_CHAT_FIXTURE_BIN:-}" ]; then
  test -x "$IRIS_STACK_CHAT_FIXTURE_BIN" || {
    echo "IRIS_STACK_CHAT_FIXTURE_BIN is not executable: $IRIS_STACK_CHAT_FIXTURE_BIN" >&2
    exit 1
  }
else
  chat_git=${IRIS_STACK_CHAT_GIT:-https://github.com/irislib/iris-chat-rs}
  chat_rev=${IRIS_STACK_CHAT_REV:-a4cafb1bb382593c9886d0a4314cf80292ef7850}
  cargo install \
    --locked \
    --root "$install_root" \
    --git "$chat_git" \
    --rev "$chat_rev" \
    --features stack-fixture \
    --bin iris-chat-stack-fixture \
    iris-chat
  IRIS_STACK_CHAT_FIXTURE_BIN=$install_root/bin/iris-chat-stack-fixture
fi

export IRIS_STACK_HTREE_BIN IRIS_STACK_DRIVE_FIXTURE_BIN IRIS_STACK_CHAT_FIXTURE_BIN
cd "$repo_dir"
cargo test --locked --test drive_htree_product -- --ignored --nocapture
cargo test --locked --test chat_drive_htree_product -- --ignored --nocapture
cargo test --locked --test relayless_mesh_product -- --ignored --nocapture
