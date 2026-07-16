#!/bin/sh
set -eu

repo_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
install_root=${IRIS_STACK_PRODUCT_INSTALL_ROOT:-"$repo_dir/target/product-lab"}
mkdir -p "$install_root"

if [ -n "${IRIS_STACK_HTREE_BIN:-}" ]; then
  test -x "$IRIS_STACK_HTREE_BIN" || {
    echo "IRIS_STACK_HTREE_BIN is not executable: $IRIS_STACK_HTREE_BIN" >&2
    exit 1
  }
else
  htree_version=${IRIS_STACK_HTREE_VERSION:-0.2.93}
  cargo install \
    --locked \
    --root "$install_root" \
    --version "=$htree_version" \
    --bin htree \
    hashtree-cli
  IRIS_STACK_HTREE_BIN=$install_root/bin/htree
fi

if [ -n "${IRIS_STACK_DRIVE_FIXTURE_BIN:-}" ]; then
  test -x "$IRIS_STACK_DRIVE_FIXTURE_BIN" || {
    echo "IRIS_STACK_DRIVE_FIXTURE_BIN is not executable: $IRIS_STACK_DRIVE_FIXTURE_BIN" >&2
    exit 1
  }
else
  : "${IRIS_STACK_DRIVE_REV:?set IRIS_STACK_DRIVE_REV to an exact published Git commit}"
  drive_git=${IRIS_STACK_DRIVE_GIT:-https://github.com/mmalmi/iris-drive}
  cargo install \
    --locked \
    --root "$install_root" \
    --git "$drive_git" \
    --rev "$IRIS_STACK_DRIVE_REV" \
    --bin iris-drive-stack-fixture \
    iris-drive-core
  IRIS_STACK_DRIVE_FIXTURE_BIN=$install_root/bin/iris-drive-stack-fixture
fi

export IRIS_STACK_HTREE_BIN IRIS_STACK_DRIVE_FIXTURE_BIN
cd "$repo_dir"
cargo test --locked --test drive_htree_product -- --ignored --nocapture
