# Iris Stack

Iris Stack is a modular set of permissionless building blocks for identity,
communication, connectivity, storage, social trust, and money. Independent
participants cooperate under the same rules without treating a platform
account, domain, IP address, payment processor, or cloud vendor as their
authority. UDP, IP, WebRTC, BLE, relays, gateways, and cloud storage remain
useful carriers; the portable protocols above them own identity, verification,
routing decisions, and settlement.

## Vision

The aim is not one decentralized replacement platform. It is a small set of
composable protocols that can replace the authority assumptions hidden in the
familiar credit-card, email-address, phone-number, IP-address, domain-name,
cloud, and message-broker stack.

Connectivity should be something people can provide wherever it is scarce.
FIPS can join ordinary Internet links, neighborhood networks, ad hoc meshes,
satellite uplinks, and future carriers without making any carrier the user's
identity. Cashu settlement can make useful forwarding and bandwidth worth
providing. That creates a path toward community-built networks and resilient
out-of-country links where centralized access is censored or unreliable.

The same pattern applies above the network. Hashtree makes stored and delivered
bytes independently verifiable, so a paid storage route can sell actual useful
cloud capacity instead of becoming the authority for the data. Nostr provides
portable identity and signed social facts. `nostr-pubsub` distributes live
events without requiring one broker. Cashu, Lightning, and Bitcoin provide
permissionless money and settlement. These capabilities should work equally
well for humans, personal agents, and services acting under explicit policy.

This is an economic direction as well as a software architecture: improve the
Bitcoin, Lightning, and Cashu foundations while making bandwidth, routing,
storage, indexing, and other measurable services exchangeable. Electricity
trading may eventually use the same identity, evidence, and settlement
primitives, but physical safety, metering, and grid control are separate
systems. Decentralized compute is another possible route type, not a missing
prerequisite for the connectivity, communication, content, and payment stack
described here.

Modularity is what keeps that ambition survivable. Every product must still
work alone and own its explicit outbound connections. Each wire protocol has
one owner. Integrations live in adapters, and a shared abstraction earns its
place by removing duplicated implementations or failure modes.

## 1. Capability layers

The order below runs from the network-facing substrate toward applications.
Nostr identity and Cashu settlement cross every layer.

| Position | Component | Role |
| --- | --- | --- |
| Identity plane | [Nostr](https://github.com/nostr-protocol/nips) | Signed events and portable public-key identity |
| 1 · datagrams | [FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips) | Authenticated public-key-addressed datagrams, encrypted links, carrier adapters, discovery, routing, and admission |
| 2 · streams | [`fips-tcp`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-tcp) | Reliable ordered delivery over FIPS |
| 3a · events | [`nostr-pubsub`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub) | Subscriptions, event exchange, deduplication, source selection, and real-time policy |
| 3b · content | [Hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree) | Hash-addressed blobs and trees, verification, caching, content routing, apps, releases, history, and Git data |
| 4a · indexes | [`@hashtree/index` and `@hashtree/collection`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree) | Immutable B-trees, canonical records, derived search roots, collection manifests, and local or federated reads |
| 4b · social context | [`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph) | Graph traversal, fact events, viewer-local naming, moderation, reputation, and resource-policy inputs |
| Settlement plane | [Cashu service layer](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service) | Bounded credit, token transfer, useful-service accounting, and settlement adapters |
| 5 · applications | [Products](#products) | User experience, authorization, durable state, economics, and explicit outbound peers |

### 1.1 Old-stack reference points

| Familiar authority | Iris primitive | Qualification |
| --- | --- | --- |
| Email, phone number, or platform account | Nostr keys, signed facts, and local or social naming | Key custody, delegation, recovery, and revocation remain application responsibilities. |
| DHCP, DNS-SD, or a service registry | Signed, expiring transport-endpoint and FIPS-capability adverts delivered through Nostr subscriptions, including `nostr-pubsub` over an existing FIPS link | FIPS authenticates every advertised candidate before use. |
| DNS or a global username registry | Nostr profile names, UUID fact-name claims, searchable indexes, and viewer-local social interpretation | Names are contextual and non-exclusive. The same string may lead different viewers to different identities. |
| IP address as identity | FIPS public-key-addressed encrypted links | IP, UDP, WebRTC, BLE, and other carriers may remain underneath. |
| TCP | `fips-tcp` reliable ordered streams | FIPS retains identity, discovery, and routing below the stream. |
| DNS name or server origin as content authority | Hashtree hashes and signed mutable roots | Domains, gateways, and cloud replicas remain useful compatibility paths. |
| Database, search service, or read-only relay index | Immutable Hashtree B-trees, collection manifests, and independently queryable search roots | The root proves the exact snapshot; the publisher determines completeness, neutrality, and update cadence. |
| Web host, domain, and TLS certificate as site identity | An immutable `nhash` or signed `npub/tree` root, delivered through an isolated browser origin | The browser origin supplies runtime isolation; the signed root identifies the app content. |
| App-store, forge, or CDN update endpoint | Signed release-root events over `nostr-pubsub`, followed by a hash-verified Hashtree artifact fetch | Relay, Blossom, and HTTPS routes can coexist with peer delivery. |
| Central message broker | `nostr-pubsub` real-time event plane | Offline history and mailboxes use separate storage. |
| Central profile, ACL, or reputation database | Signed fact events and locally interpreted social graphs | Each viewer interprets signed evidence through local policy. |
| Credit card or platform balance | Cashu, Lightning, Bitcoin, and useful-service receipts | Mint trust and cross-mint settlement remain explicit concerns. |

## 2. Connectivity

### 2.1 FIPS authenticated datagrams

[FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips)
gives each node a self-generated public-key address and authenticated, encrypted
datagrams. Peer links can use UDP, TCP, Ethernet, Tor, BLE, WebRTC, or other
carriers, and several transports can participate in one routed mesh.

Applications address a FIPS identity while the node handles peer discovery,
path selection, forwarding, admission, and link health. An IPv6 adapter lets
existing IP software reach the same identities; native applications use FIPS
service datagrams directly.

### 2.2 Reliable streams with fips-tcp

[`fips-tcp`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-tcp)
adds reliable, ordered byte streams above FIPS datagrams. Applications gain
connected stream semantics, flow control, and congestion control while the
remote address remains an authenticated FIPS identity. It suits protocols that
need a continuous connection; event exchange and hash-addressed blobs can use
their own higher-level delivery models.

## 3. Identity, events, and social context

### 3.1 Nostr identity and signed facts

Nostr keys provide the portable identity shared by FIPS nodes and application
events. Signed events carry profiles, follows, mutes, messages, capability
adverts, ratings, release roots, and application-defined facts. Signatures bind
an event to its author; each application decides what that author may do.

The [`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph)
identity tools add UUID-based rosters and facts for identities that span several
keys or devices. They support key changes without changing every reference to
the identity.

### 3.2 nostr-pubsub event distribution

[`nostr-pubsub`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub)
carries ordinary Nostr subscriptions and signed events across local indexes,
FIPS peers, mesh peers, and optional relays. An application subscribes once and
applies local policy when choosing sources or accepting events.

The same event plane carries social posts and stack coordination events:
peer adverts, machine ratings, Hashtree roots, app updates, repository
announcements, and service offers. Events announce large content by hash;
Hashtree routes carry the bytes.

### 3.3 Peer discovery from one FIPS link

An authenticated FIPS connection is a bootstrap path. Through
`nostr-pubsub-fips`, an application sends an ordinary Nostr subscription and
its peer returns or forwards signed, expiring adverts:

| Question | How the answer is used |
| --- | --- |
| Which IP, UDP, WebRTC, or other transport endpoint can I connect FIPS to? | Treat the endpoint and claimed identity as a candidate, then authenticate it with FIPS. |
| Which FIPS identity offers Hashtree, `nostr-pubsub`, or another capability? | Connect to or reuse that peer, then use the advertised interface. |

The DHCP reference applies to endpoint discovery: *which IP endpoint can I
connect FIPS to?* Capability adverts also answer *which FIPS identity offers
this service?* FIPS authenticates each candidate, and local author, social,
capability, and resource policy decides admission. Relays, indexes, and other
pub/sub routes can answer the same request.

### 3.4 Social graph as local policy

[`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph)
turns follows, mutes, signed facts, and ratings into viewer-relative signals.
Each app chooses whose signals to trust, how far they travel, and how to handle
unknown identities. There is no global trust score.

| Input | Local decision |
| --- | --- |
| Follows, social distance, and mutes | Prioritize ordinary Nostr posts from nearby authors; filter feeds, replies, notifications, profiles, and search results. |
| Machine observations and signed peer or service ratings | Prefer healthy FIPS and pub/sub peers, downrank degraded sources, and preserve exploration space for unknown peers. |
| UUID identity rosters, facts, and attestations | Keep identity consistent across keys and devices; rank contextual names and decide which claims an app accepts. |

[`nostr-pubsub-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub/crates/nostr-pubsub-social-graph)
adapter uses social distance, nearby mutes, and signed service ratings to allow,
throttle, drop, or prioritize Nostr event authors and pub/sub peers. Nostr VPN
uses this policy for peer and event admission. [FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips)
publishes machine-generated peer ratings and can use selected rating authors to
order candidate peers during discovery.

[Hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree)
uses the graph for crawl scope, relay and storage access, mirror selection, and
profile search. Iris Contacts, Drive, Git, and Social use it to rank profiles,
search results, feeds, and sharing contacts.

The same signal can feed resource schedulers. A Hashtree node can serve or
fetch for socially close or reputable peers first; a FIPS host can reserve
connection slots or bandwidth for them. Each node controls this scheduling and
can reserve capacity for unfamiliar peers.

### 3.5 Human names without a global namespace

Cryptographic keys are secure addresses but poor human names. Naming is a
signed search problem:

- Nostr profiles propose names for public keys.
- Fact events attach `name` claims to durable UUIDs and `controls` claims to
  keys, preserving identity while keys or names change.
- Hashtree stores provide exact tag lookup; `@hashtree/index` or another
  database can add tokenized search.
- Search returns candidates. Viewer-local follows, social distance, explicit
  trust, and application policy rank them.

Names are signed, non-exclusive claims. The same string can identify several
candidates, and publishing it first grants no exclusive claim. Each viewer
resolves it through accepted authors, social context, and private petnames while
the key or UUID remains stable.

This follows the petname approach described in
[Zooko's original naming essay](https://www.cs.princeton.edu/courses/archive/spr17/cos518/papers/zooko-triangle.pdf)
and [An Introduction to Petname Systems](https://www.skyhunter.com/marcs/petnames/IntroPetNames.html):
published nicknames aid discovery; a name becomes a petname only when a viewer
adopts a private, unambiguous mapping to a secure identity.

Hashtree-backed profile search and social ranking are available today.
`nostr-social-memory` supports UUID identities with petnames, aliases, and
multiple keys; a unified fact-name resolver remains to be integrated.
[Iris Contacts](https://contacts.iris.to/) combines profiles, graph-ranked
search, and its own UUID-backed contacts encoded as fact snapshots; its
[source is on Iris Git](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-contacts).

## 4. Verifiable content and indexes

### 4.1 Hashtree blobs and routes

Hashtree presents one operation—fetch a blob by hash—to every possible source.
Anything that can answer is a `BlobRoute`.

Local storage, nearby peers, the wider mesh, and paid providers can all answer
through that interface. Every response is verified against the requested hash.
A route miss leaves the other routes available. `nostr-pubsub` announces
content; Hashtree routes carry the blobs.

### 4.2 Hashtree indexes for large datasets

- [`@hashtree/index`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-index)
  stores immutable B-trees for exact, range, prefix, and lightweight text
  queries over a content root.
- [`@hashtree/collection`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-collection)
  adds canonical by-ID records, derived key/search roots, and a manifest.
- An indexer may curate centrally and publish snapshots through a signed
  mutable `npub/tree`; readers can fetch blocks from caches, FIPS peers, or
  compatible stores, verify them, query locally, and mirror or fork the root.

This supports reproducible Nostr-relay-like read projections. The publisher
defines the available queries and update cadence; applications can query or
federate several compatible publishers.

[Iris Audio](https://audio.iris.to/) demonstrates the model with shared song,
artist, and album search roots queried directly by the browser.
[Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-audio).

### 4.3 Web apps and updates as verified trees

A static web app can be published as a Hashtree directory. Its `nhash`
identifies one immutable version; a signed `npub/tree` name can advance to a
new version without invalidating the old one.

Iris Drive serves executable sites from separate browser origins such as
`sitename.npub.iris.localhost` or `<nhash>.iris.localhost`. The separation keeps
unrelated apps from sharing cookies, storage, or service workers. Application
sandboxing remains a separate concern.

[Iris Sites](https://apps.iris.to/) lists and launches these apps; the older
[`sites.iris.to`](https://sites.iris.to/) address redirects there. The catalog
aids discovery, while Hashtree roots authenticate the app bytes.
[Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-sites).

`hashtree-updater` applies the same model to native releases. A signed root
arrives through `nostr-pubsub`; the app checks the publisher, fetches the
release through its Hashtree routes, verifies the content address, and installs
the matching artifact.

Runtime updates can receive both notice and bytes from stack-native peers, with
relay, Blossom, and HTTPS compatibility routes available.
[`hashtree-updater` source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/rust/crates/hashtree-updater).

## 5. Settlement

### 5.1 Cashu service layer

[`cashu-service`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service)
provides Cashu token transfer and settlement together with sat-denominated
useful-service receipts and bounded peer credit. A node can accept a peer's
credit up to a local limit, then request settlement through an accepted Cashu
mint, Lightning, or another configured method.

Products choose pricing, credit limits, and accepted settlement methods. The
same adapters can account for connectivity, bandwidth, storage, routing, or
other services, while free and reciprocal routes remain available.

## 6. Products

The broader app catalog is [sites.iris.to](https://sites.iris.to/); it lists many
apps beyond the examples below.

| Product | User-facing value | Links |
| --- | --- | --- |
| Iris Chat | Encrypted, local-first messaging without phone-number or email signup | [Product page](https://irischat.org/) · [Web app](https://chat.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-chat) |
| Iris Drive | Offline-first, content-addressed file sync and collaboration across devices, peers, and optional storage providers | [Product page](https://getdrive.iris.to/) · [Web app](https://drive.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-drive-web) |
| Nostr VPN | A private mesh that connects directly when possible, and a public exit-node marketplace when you need an internet route | [Product page](https://nostrvpn.org/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-vpn) |
| Iris Contacts | Public-key profiles, social context, and local UUID-backed contacts without a global account directory | [Web app](https://contacts.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-contacts) |
| Iris Audio | A Hashtree-backed music catalog that demonstrates portable collection and search indexes | [Web app](https://audio.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-audio) |
| Iris Sites | A launcher and isolated browser runtime for web apps published as Hashtree trees | [App catalog](https://sites.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-sites) |
| Iris Git | Git repositories addressed through Nostr and Hashtree instead of one forge account or origin server | [Web app](https://git.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-git) |

## 7. Repository index

| Component | Source |
| --- | --- |
| Iris Stack architecture | [iris-stack](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-stack) · [GitHub mirror](https://github.com/irislib/iris-stack) |
| Nostr specifications | [nostr-protocol/nips](https://github.com/nostr-protocol/nips) |
| FIPS (Rust) | [fips](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips) · [GitHub mirror](https://github.com/mmalmi/fips) |
| FIPS (TypeScript) | [fips-ts](https://github.com/mmalmi/fips-ts) |
| Reliable FIPS streams | [fips-tcp](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-tcp) · [GitHub mirror](https://github.com/mmalmi/fips-tcp) |
| Decentralized pub/sub | [nostr-pubsub](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub) · [GitHub mirror](https://github.com/mmalmi/nostr-pubsub) |
| Content-addressed storage and routing | [hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree) · [GitHub mirror](https://github.com/mmalmi/hashtree) |
| Content-addressed search indexes | [`@hashtree/index`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-index) · [GitHub mirror](https://github.com/mmalmi/hashtree/tree/master/ts/packages/hashtree-index) |
| Content-addressed collections | [`@hashtree/collection`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-collection) · [GitHub mirror](https://github.com/mmalmi/hashtree/tree/master/ts/packages/hashtree-collection) |
| App updates over Hashtree | [`hashtree-updater`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/rust/crates/hashtree-updater) · [GitHub mirror](https://github.com/mmalmi/hashtree/tree/master/rust/crates/hashtree-updater) |
| Social graph and fact events | [nostr-social-graph](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph) · [GitHub mirror](https://github.com/mmalmi/nostr-social-graph) |
| Cashu service primitives | [cashu-service](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service) |
| Private-message ratchet | [nostr-double-ratchet](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-double-ratchet) · [GitHub mirror](https://github.com/irislib/nostr-double-ratchet) |
| Shared web integration | [iris-kit](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-kit) · [GitHub mirror](https://github.com/mmalmi/iris-kit) |
