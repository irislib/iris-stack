# Integration lab

The executable in this repository is a thin black-box lab for same-host Iris
composition. It implements no FIPS discovery or handshake, FIPS TCP framing,
Hashtree blob protocol, routing, retry, or caching. It exercises those
capabilities exclusively through their production APIs.

## Process topology

Each test starts independent operating-system processes with independent FIPS
identities:

```text
                         application-owned UDP links
anchor ------------------------------------------------------\
provider -----------------------------------------------------+-- external peer
consumer ----------------------------------------------------/

anchor <==== authenticated loopback FIPS ====> provider
   ^                                              ^
   +======== authenticated loopback FIPS ========+-- consumer
```

The first local process exclusively binds an isolated loopback UDP address.
Other processes know the address but not its identity. FIPS obtains an
untrusted public-key hint and proves it with the ordinary Noise IK handshake.
The lab treats that exchange as a black box and observes the resulting
authenticated peer and capability directory.

The provider wraps an ordinary Hashtree store in `SameHostBlobStore` and
advertises `hashtree.blob/1` through authenticated FSP. The consumer fetches a
blob through the production FIPS TCP adapter, verifies its hash, and caches it.

## Failure matrices

The process tests cover:

- graceful and forced rendezvous-anchor death, exclusive rebinding, and local
  topology convergence;
- graceful and forced provider death, capability withdrawal, replacement, and
  an uncached fetch from the replacement identity;
- a local cache hit after provider disappearance;
- route-local `NoResult` before a provider exists and after it disappears;
- application-owned direct UDP traffic before, during, and after local churn.

The last check is important: same-host reuse does not suppress or delegate a
product's outbound connections. `hashtree.blob/1` is a direct provider
interface, not a replacement for Hashtree HTL routing.

## Released-product gate

The substrate matrix above is fast and deterministic, but it is not sufficient
evidence that a product composes the released pieces correctly. The explicit
`drive_htree_product` gate starts these real processes:

```text
remote htree (released CLI, owns UDP FIPS link)
              ^
              | Hashtree HTL over FIPS TCP
              |
local htree provider <== ordinary loopback FIPS ==> Iris Drive fixture
              ^                                      |
              |                                      |
              +---------- own UDP FIPS links --------+
```

The fixture lives in the Iris Drive repository and only controls its production
`FipsBlockSync`; it contains no discovery, transport, blob, or routing
implementation. The lab gives every process its own identity, storage, UDP
port, and isolated loopback rendezvous address. Hashtree LAN discovery is
disabled so a passing run cannot silently use a host-LAN path. Hashtree keeps
its generic overlay scope while Drive keeps its profile scope; authenticated
same-host capability discovery is intentionally cross-product.

The scenario seeds two trees in the remote `htree` process. Drive retrieves the
first through the already-running local provider, which continues the request
through its Hashtree resolver and caches the result. The lab then kills the
provider and retrieves the second tree through Drive's pre-existing,
application-owned UDP route. It verifies the first tree is durable in the dead
provider's store, the second was not supplied by that provider, and Drive's UDP
route remains authenticated and connected before and after the failure.

This is intentionally one product slice. Iris Drive still owns its product
authorization and startup tests; Hashtree still owns HTL, codec, failure, and
resource-bound tests. Later Chat, VPN, and Git scenarios should reuse the same
generic process harness rather than add protocol simulators here.

## Run the lab

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

The released-product gate is ignored by ordinary `cargo test` because it
installs and runs external artifacts. Run it with an exact public Iris Drive
commit; Hashtree is installed from the exact registry version unless a binary
is supplied explicitly:

```sh
IRIS_STACK_DRIVE_REV=<exact-commit> \
scripts/product-lab.sh
```

For pre-release verification, provide both candidate executables without
patching this repository or its lockfile:

```sh
IRIS_STACK_HTREE_BIN=/path/to/htree \
IRIS_STACK_DRIVE_FIXTURE_BIN=/path/to/iris-drive-stack-fixture \
scripts/product-lab.sh
```

`.github/workflows/product-lab.yml` exposes the same gate as a manually
dispatched or reusable workflow. Both inputs are exact artifact coordinates;
the workflow has no checkout-relative references to sibling repositories.
