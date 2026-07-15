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

## Run the lab

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```
