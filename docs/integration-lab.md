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

The checked-in Rust lockfile pins the substrate gate to the published
`fips-core` 0.4.1, `fips-tcp` 0.2.0, and `hashtree-fips-transport` 0.4.1
artifacts. Product fixtures and the `htree` executable are supplied as exact
coordinates at run time.

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

## Cashu service-payment recovery gate

The ordinary test suite also composes the published `cashu-service` 0.3.1 and
`cashu-credit` 0.3.0 artifacts through a real loopback CDK mint API and SQLite
wallets. Its isolated payment network simulates only the Lightning backend; CDK
still owns quotes, proofs, swaps, melts, bearer-token transfer, and spent-proof
checks. Building CDK requires a `protoc` executable; the CI workflow installs
the operating system's protobuf compiler explicitly.

The lab starts a withdrawable source mint and a closed-loop provider mint, then
runs payer, replacement-payer, provider, and replay-receiver roles as separate
operating-system processes. It proves this bounded sequence:

1. A persisted useful-service receipt authorizes one 32 sat payout with a one
   sat fee ceiling from 64 funded sats.
2. Taking the destination mint offline makes the attempt fail without moving
   value or erasing the pending authorization.
3. After the mint returns, the payer completes the CDK transfer and durably
   journals one exact bearer token, then is killed before receiver
   acknowledgement.
4. A replacement process resumes the same expired authorization and CDK saga,
   returning the same quote IDs and replaying the same token journal rather
   than issuing another payment.
5. The provider receives 32 sats; a second wallet's replay of the same token is
   rejected by the real mint. Only then does the replacement payer complete
   the credit-account settlement.
6. Final accounting remains conserved: 31 sats at the source mint, 32 at the
   destination mint, and one sat in the fee sink from 64 externally funded
   sats.

This gate verifies generic useful-service settlement recovery. It does not
claim integration with paid FIPS forwarding, Hashtree storage, or VPN traffic
metering; each product must still bind its authenticated service effect to the
receipt and receiver acknowledgement.

## Nostr VPN released-product gate

The VPN product gate is a thin launcher for Nostr VPN's owner-repository
process test. By default it fetches exact public commit
[`bb9de1977d732757bc90315aea24f8fbfce2765e`](https://github.com/mmalmi/nostr-vpn/commit/bb9de1977d732757bc90315aea24f8fbfce2765e)
into a temporary checkout and runs its canonical
`scripts/e2e-connect-docker.sh`. Iris Stack does not copy the VPN topology,
commands, protocol, or assertions.

That owner harness starts two real `nvpn` processes with separate identities
and explicit UDP roster endpoints. It proves that both application-owned links
carry traffic in both directions while the same processes deliver a signed
kind-37196 paid-exit event through the shared TCP/FIPS pubsub service. The gate
does not introduce local route delegation, shared egress, or a mandatory
same-host process.

The gate is intentionally opt-in because it builds and runs privileged network
containers. Run the pinned coordinate with:

```sh
scripts/vpn-product-lab.sh
```

Pre-release candidates can be selected without editing this repository:

```sh
IRIS_STACK_NVPN_REV=<exact-commit> \
IRIS_STACK_NVPN_GIT_URL=<public-git-url> \
scripts/vpn-product-lab.sh
```

`.github/workflows/vpn-product-lab.yml` exposes only manual and reusable
workflow entry points. Ordinary pushes and pull requests do not incur the
Docker gate's cost.

## Run the lab

```sh
cargo test --locked
cargo test --locked --test cashu_payment_product -- --nocapture
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

The released-product gate is ignored by ordinary `cargo test` because it
installs and runs external artifacts. The script pins Iris Drive commit
`142ea1e83b5251d1fbdf6e9a0ce44126892d2fbc` and `hashtree-cli` 0.2.85 by
default. Run it directly, or override either exact coordinate explicitly:

```sh
scripts/product-lab.sh

IRIS_STACK_DRIVE_REV=<exact-commit> \
IRIS_STACK_HTREE_VERSION=<exact-version> \
scripts/product-lab.sh
```

For pre-release verification, provide both candidate executables without
patching this repository or its lockfile:

```sh
IRIS_STACK_HTREE_BIN=/path/to/htree \
IRIS_STACK_DRIVE_FIXTURE_BIN=/path/to/iris-drive-stack-fixture \
scripts/product-lab.sh
```

`.github/workflows/product-lab.yml` runs the Cashu gate on relevant pushes and
pull requests. It also exposes the Drive/Hashtree gate as a manually dispatched
or reusable workflow. Its optional product inputs override the script's exact
artifact coordinates; the workflow has no checkout-relative references to
sibling repositories.
