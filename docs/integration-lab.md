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

provider Store.put ======> shared PoolStore <====== consumer BlobRouter.get
replacement Store.put ============^
```

The first local process exclusively binds an isolated loopback UDP address.
Other processes know the address but not its identity. FIPS obtains an
untrusted public-key hint and proves it with the ordinary Noise IK handshake.
The lab treats that exchange as a black box.

Provider, consumer, and replacement processes independently open one configured
Hashtree data directory through the canonical shared-LMDB opener, with an
identical storage budget. A fresh directory must open as one `PoolStore`; each
process sees the pool as one application-owned `Store`, while the pool alone owns
member placement and adaptation. Providers write immutable blobs explicitly
through that store. The consumer reads through a production `BlobRouter`
containing an ordinary `StoreBlobRoute`; the router performs the central hash
check. FIPS is not in this local read path and no local process is a mandatory
daemon. This direct sharing applies to immutable blob bytes plus the pool's
explicit transactional catalog. The lab does not treat product metadata,
identities, or indexes as implicitly shareable application state.

## Failure matrices

The process tests cover:

- graceful and forced rendezvous-anchor death while shared reads and each
  process's direct egress continue;
- graceful and forced provider death followed by a distinct replacement
  process writing through a separately opened LMDB handle;
- a long-lived reader observing new immutable blobs from the original and
  replacement writers, and retaining the original blob after writer death;
- route-local `NoResult` before the requested hash is written, without treating
  that result as global absence;
- application-owned direct UDP traffic before, during, and after local churn.

The last check is important: same-host reuse does not suppress or delegate a
product's outbound connections. Direct shared storage is one opaque blob route,
not a replacement for Hashtree HTL routing.

## Released-product gate

The substrate matrix above is fast and deterministic, but it is not sufficient
evidence that products compose the released pieces correctly. Three explicit
released-product gates cover the combined Chat/Drive topology, Drive's
provider-replacement lifecycle, and native relayless transit and recovery.

The checked-in Rust lockfile pins the substrate gate to the published
`nvpn-fips-core` 0.4.78, `nvpn-fips-tcp` 0.2.2, and
`hashtree-fips-transport` 0.4.17 artifacts while retaining `hashtree-core`
0.2.89. Product fixtures and the separately released `htree` executable are
supplied as exact coordinates at run time.

```text
HTL-only remote htree <== provider-owned UDP FIPS ==> local htree provider
                                                       /              \
                                        loopback FIPS /                \ loopback FIPS
                                                     /                  \
                                            Iris Chat fixture      Iris Drive fixture
                                                     |                  |
                                         configured HTTP source     app-owned UDP FIPS
                                                     |                  |
                                                     +-- standalone htree
```

The feature-gated fixtures live with Iris Chat and Iris Drive. They invoke
Chat's production attachment reader and Drive's production `FipsBlockSync`;
neither fixture implements discovery, transport, blob framing, or routing. The
lab gives every process its own identity, storage, UDP port, and isolated
loopback rendezvous address. Hashtree LAN discovery is disabled so a passing
run cannot silently use a host-LAN path.

The scenario first puts one blob only in the local provider and proves that
both products retrieve it through authenticated same-host discovery. A second
blob exists only at the provider's one configured Hashtree peer. Drive sends
`BLOB_DEFAULT_HTL` to the same-host provider; the provider performs the one
Hashtree mesh forwarding hop with the budget decremented from 10 to 9, verifies
the reply, and caches it. The lab proves the standalone remote never held that
blob and that the provider cache can serve it afterward. The exact wire-budget
invariant remains owned by Hashtree's
`test_blob_request_forwarding_decrements_htl_exactly_once` test.

The remaining remote-only blobs exist only at the apps' standalone remote. A
provider search that returns `NoResult` remains route-local: Chat falls through
to its configured HTTP source while Drive keeps using its pre-existing,
application-owned UDP route. After a forced provider death, both products
retrieve another blob through those same standalone paths. The lab verifies
the dead provider never acquired either fallback blob, Drive's UDP route stays
authenticated and connected throughout, and Chat remains alive. Nostr pub/sub
is not involved in blob routing.

The separate Drive lifecycle scenario then starts a distinct local provider,
kills it, and replaces it with another process and identity. It proves old and
replacement-only blobs remain available through the correct stores while
Drive's application-owned UDP route remains authenticated throughout.

## Native relayless mesh gate

`relayless_mesh_product` uses the same three released product executables to
exercise an isolated native mesh:

```text
Iris Chat A <==== UDP FIPS ====> htree B <==== UDP FIPS ====> Iris Drive C
              signed events and verified blobs between A and C
```

Each process has a separate identity, store, UDP port, and loopback rendezvous
address. A and C know each other's identities but only B's physical address.
The gate checks that B is their sole direct FIPS peer. Public relays, WebSocket
seeds, WebRTC, and host-LAN discovery are disabled. B runs the ordinary Hashtree
daemon with Nostr disabled, so it has no event subscription or Chat/Drive
application membership.

The process fixture commands call the products' production pubsub and attachment
APIs. The lab verifies signed event IDs, authors, kinds, and contents in both
directions, and compares a 192 KiB attachment with its original SHA-256. It then
forcibly stops B, publishes one new event at each still-running endpoint and
stores a fresh blob in Drive, then starts a replacement B with the original
transit identity and addresses.
The endpoints must receive the original event IDs through their original
subscriptions, without being restarted or manually republishing. A fresh blob
must also transfer after rejoin. Timings are printed for event delivery, blob
retrieval, and recovery.

Before partition and after recovery, the gate allows three seconds for final
acknowledgements, then samples fifteen idle seconds. It reports cumulative CPU
time for each process as a percentage of one core, and reads Hashtree's native
per-peer byte counters. Combined transit send/receive traffic must stay below
4 KiB/s, allowing ordinary keepalives and routing maintenance while detecting
continued payload or retry storms. Each process must stay at or below 5% of one CPU
core; `IRIS_STACK_IDLE_MAX_CPU_PERCENT` explicitly overrides that generous
hot-loop budget for a different runner. Unsupported CPU sampling is reported
as unavailable. Linux reads `/proc` CPU ticks, while macOS reads cumulative
`ps` CPU time; these remain short regression samples, not precise benchmarks.
The checks cover native mesh processes, not browser or UI CPU.
The transit must have two live peers at both counter samples. Missing or
malformed byte/packet counters, counter resets, process exits, and CPU reader
errors fail the gate. Only an unsupported CPU reader or unavailable sampling
tool skips CPU enforcement; the result explicitly prints `unavailable`, and
the traffic checks still run.

These idle checks are part of the standard `scripts/product-lab.sh` command;
there is no additional resource-test switch. The released-product workflow
runs that command automatically on pushes and pull requests affecting its
workflow, launcher, Cargo manifests/lockfile, the listed product tests, or shared
test support. It also supports manual dispatch and reusable workflow calls.
Ordinary `cargo test --all-targets` leaves the external product tests ignored,
so the generic native CI workflow alone does not enforce the idle budgets.
This is a regression gate on those runs, not continuous runtime monitoring.
Upstream Chat, Drive, and Hashtree release commands do not currently invoke this
cross-product gate automatically; a release coordinator runs the standard
command or calls the reusable workflow with the new public artifact pins.

Three serial native runs on 2026-09-09 used the pinned Chat and Drive source
fixtures and Hashtree 0.2.146, built with their locked registry dependencies.
All runs delivered four signed events and two verified blobs with direct peer
counts A=1, B=2, C=1 and no public relays. The default resource limits were enabled
and every CPU reading was available:

| Run | Initial event pair | Initial blob | Restart to queued events | Fresh blob | Idle transit before / after | Highest process CPU |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 115 ms | 236 ms | 13.750 s | 146 ms | 2653 / 3108 B/s | 1.33% |
| 2 | 74 ms | 227 ms | 13.569 s | 129 ms | 2566 / 2928 B/s | 1.53% |
| 3 | 92 ms | 222 ms | 13.587 s | 134 ms | 2565 / 2939 B/s | 1.47% |

These short macOS samples used debug fixture executables. They establish
functional recovery and headroom under the regression budgets, rather than
an optimized-build throughput benchmark.

The launcher drains process output continuously, including when no fixture
command is pending, so verbose transit logs cannot block its network runtime.

The blob stream in this scenario is end-to-end between A and C over FIPS. B
forwards encrypted packets and does not cache that stream. The preceding
Chat/Drive/Hashtree gate separately tests Hashtree application-level HTL
forwarding and caching. Neither gate claims browser connectivity, discovery
without any contact hints, or recovery of an endpoint's outbox after its own
process dies.

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
[`81c1dee7664a85af4d153f856bc0f4f277ba2e29`](https://github.com/mmalmi/nostr-vpn/commit/81c1dee7664a85af4d153f856bc0f4f277ba2e29)
into a temporary checkout and runs its canonical
`scripts/e2e-connect-docker.sh`. Iris Stack does not copy the VPN topology,
commands, protocol, or assertions.

That owner harness starts two real `nvpn` processes with separate identities
and explicit UDP roster endpoints. It proves that both application-owned links
carry traffic in both directions while the same processes deliver a signed
kind-37196 paid-exit event through the shared TCP/FIPS pubsub service. The gate
does not introduce local route delegation, shared egress, or a mandatory
same-host process.

The gate builds and runs privileged network containers. Run the pinned
coordinate directly with:

```sh
scripts/vpn-product-lab.sh
```

Pre-release candidates can be selected without editing this repository:

```sh
IRIS_STACK_NVPN_REV=<exact-commit> \
IRIS_STACK_NVPN_GIT_URL=<public-git-url> \
scripts/vpn-product-lab.sh
```

`.github/workflows/vpn-product-lab.yml` remains manually dispatchable and
reusable. It also runs on pushes and pull requests that change the workflow or
its launcher, so unrelated source and documentation changes do not incur the
Docker gate's cost.

## Run the lab

```sh
cargo test --locked --all-targets
cargo test --locked --test cashu_payment_product -- --nocapture
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

The released-product gate is ignored by ordinary `cargo test` because it
installs and runs external artifacts. The script pins Iris Drive commit
`05751a828f2a20b3ed46d09569e6cf35ac4c537d`, `hashtree-cli` 0.2.146, and Iris Chat
commit `a4cafb1bb382593c9886d0a4314cf80292ef7850` by default. Chat's native
release uses a date tag, so its fixture is built from the exact public source
commit with its locked registry dependencies. Drive is fetched from its public
canonical Hashtree repository through the released `git-remote-htree` helper
and Cargo's Git CLI transport. The Hashtree CLI installation includes that
helper; supplying only an external `htree` binary installs the helper separately
when Drive still needs to be built. Run the lab directly, or override an exact
coordinate explicitly:

```sh
scripts/product-lab.sh

IRIS_STACK_DRIVE_REV=<exact-commit> \
IRIS_STACK_HTREE_VERSION=<exact-version> \
IRIS_STACK_CHAT_REV=<exact-commit> \
scripts/product-lab.sh
```

The pinned Drive source route was independently checked with `cargo install
--debug --locked`, an empty Cargo Git cache, and an empty anonymous Hashtree
store with the local daemon disabled. Cargo resolved the exact commit, built
and installed the fixture, and its identity command passed. This checks source
retrieval and installation; the default launcher uses Cargo's release profile.

For pre-release verification, provide the three candidate executables without
patching this repository or its lockfile:

```sh
IRIS_STACK_HTREE_BIN=/path/to/htree \
IRIS_STACK_DRIVE_FIXTURE_BIN=/path/to/iris-drive-stack-fixture \
IRIS_STACK_CHAT_FIXTURE_BIN=/path/to/iris-chat-stack-fixture \
scripts/product-lab.sh
```

`.github/workflows/product-lab.yml` runs the Cashu and Chat/Drive/Hashtree gates
on relevant pushes and pull requests, and remains manually dispatchable and
reusable. Its optional product inputs override the script's exact artifact
coordinates; the launcher owns the default version pins so local and CI runs
cannot drift. The workflow has no checkout-relative references to sibling
repositories. The generic native workflow runs `cargo test --locked
--all-targets`, including `same_host_processes`, on every push and pull request.

## Site release verification

`pnpm run release:site` accepts only `hashtree-cli` 0.2.100, either from `PATH`
or from an explicit executable `HTREE_BIN`. After the portable build and tests,
Hashtree publication and Cloudflare deployment run independently. Once the
publisher emits an immutable `nhash`, the release script uses a fresh
`HTREE_CONFIG_DIR` and data directory to retrieve it and compares every path
and file byte with `dist`.

The two publication targets are not an atomic transaction: one can have
changed when the other fails. The command is nevertheless fail-closed. A bad
exit status, missing publication reference, fresh-store retrieval failure, or
byte mismatch prevents a success result and must be investigated before the
release is reported complete.
