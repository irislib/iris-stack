#!/bin/sh
set -eu

case "${CANDIDATE_MODE:-source}" in
package)
  : "${FIPS_IDENTITY_CRATE_DIR:?set every packaged candidate crate directory}"
  : "${FIPS_CORE_CRATE_DIR:?set every packaged candidate crate directory}"
  : "${FIPS_TCP_CRATE_DIR:?set every packaged candidate crate directory}"
  : "${FIPS_TCP_ENDPOINT_CRATE_DIR:?set every packaged candidate crate directory}"
  : "${HASHTREE_CORE_CRATE_DIR:?set every packaged candidate crate directory}"
  : "${HASHTREE_FIPS_TRANSPORT_CRATE_DIR:?set every packaged candidate crate directory}"
  required_manifest=Cargo.toml.orig
  ;;
source)
  : "${FIPS_DIR:?set FIPS_DIR to the FIPS source tree}"
  : "${FIPS_TCP_DIR:?set FIPS_TCP_DIR to the fips-tcp source tree}"
  : "${HASHTREE_DIR:?set HASHTREE_DIR to the Hashtree source tree}"
  FIPS_IDENTITY_CRATE_DIR=$FIPS_DIR/crates/fips-identity
  FIPS_CORE_CRATE_DIR=$FIPS_DIR/crates/fips-core
  FIPS_TCP_CRATE_DIR=$FIPS_TCP_DIR/rust/fips-tcp
  FIPS_TCP_ENDPOINT_CRATE_DIR=$FIPS_TCP_DIR/rust/fips-tcp-endpoint
  HASHTREE_CORE_CRATE_DIR=$HASHTREE_DIR/rust/crates/hashtree-core
  HASHTREE_FIPS_TRANSPORT_CRATE_DIR=$HASHTREE_DIR/rust/crates/hashtree-fips-transport
  required_manifest=Cargo.toml
  ;;
*)
  echo "CANDIDATE_MODE must be source or package" >&2
  exit 1
  ;;
esac

for crate_dir in \
  "$FIPS_IDENTITY_CRATE_DIR" \
  "$FIPS_CORE_CRATE_DIR" \
  "$FIPS_TCP_CRATE_DIR" \
  "$FIPS_TCP_ENDPOINT_CRATE_DIR" \
  "$HASHTREE_CORE_CRATE_DIR" \
  "$HASHTREE_FIPS_TRANSPORT_CRATE_DIR"
do
  test -f "$crate_dir/$required_manifest" || {
    echo "missing candidate artifact: $crate_dir/$required_manifest" >&2
    exit 1
  }
done

# Registry-normalized locks cannot also record path-patched package sources.
# Let Cargo normalize a disposable copy for the selected source/package tuple,
# then keep the actual test and lint invocations locked to that copy.
candidate_workspace=$(mktemp -d "${TMPDIR:-/tmp}/iris-stack-candidates.XXXXXX")
trap 'rm -rf "$candidate_workspace"' EXIT
cp Cargo.toml Cargo.lock "$candidate_workspace/"
cp -R src tests "$candidate_workspace/"
candidate_manifest=$candidate_workspace/Cargo.toml

candidate_cargo() {
  subcommand=$1
  shift
  CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$PWD/target"} cargo "$subcommand" \
    --manifest-path "$candidate_manifest" \
    --config "patch.crates-io.fips-identity.path='$FIPS_IDENTITY_CRATE_DIR'" \
    --config "patch.crates-io.fips-core.path='$FIPS_CORE_CRATE_DIR'" \
    --config "patch.crates-io.fips-tcp.path='$FIPS_TCP_CRATE_DIR'" \
    --config "patch.crates-io.fips-tcp-endpoint.path='$FIPS_TCP_ENDPOINT_CRATE_DIR'" \
    --config "patch.crates-io.hashtree-core.path='$HASHTREE_CORE_CRATE_DIR'" \
    --config "patch.crates-io.hashtree-fips-transport.path='$HASHTREE_FIPS_TRANSPORT_CRATE_DIR'" \
    "$@"
}

cargo fmt --check
candidate_cargo metadata --offline --format-version 1 >/dev/null
candidate_cargo test --locked --test same_host_processes -- --nocapture
candidate_cargo clippy --locked --all-targets -- -D warnings
