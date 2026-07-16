# Iris Stack

## A Freedom Tech Toolkit

Iris Stack is a modular, permissionless stack for identity, communication,
connectivity, storage, social trust, and money. Its FIPS connectivity layer can
use carrier adapters at multiple layers: Ethernet, Bluetooth LE, UDP, TCP,
WebRTC, Tor, or relays. Nostr provides
portable signed identity and publish-subscribe messaging; Hashtree verifies
content by hash; viewer-local social graphs supply trust policy; and Cashu
provides credit and settlement.
Applications combine these layers without treating a platform account, domain,
or cloud vendor as an authority.

<figure class="app-catalog">
  <a href="https://apps.iris.to/"><img src="./apps-iris-to.png" width="960" height="794" alt="Browse the Iris app catalog"></a>
  <figcaption><a href="https://apps.iris.to/">apps.iris.to</a> catalogs <a href="#products">Iris Stack products</a>, including Iris Drive, Chat, Calendar, and Nostr VPN. The <a href="https://getdrive.iris.to/">Iris Drive native app</a> can resolve Hashtree-published apps through the local <code>iris.localhost</code> resolver and cache their files for offline launch; features that depend on peers or external services still require connectivity.</figcaption>
</figure>

## 1. Capability layers

The order below runs from the network-facing substrate toward applications.
Nostr identity and Cashu settlement cross every layer.

| Position | Component | Role | Replaces |
| --- | --- | --- | --- |
| [Identity plane](#nostr-identity-and-signed-facts) | Nostr | Portable public-key identity and signed event format | Platform account, email address, phone number, domain name, or TLS certificate as identity |
| [1 · datagrams](#fips-authenticated-datagrams) | FIPS | Authenticated datagrams addressed by self-generated public keys, encrypted links, carrier adapters, discovery, routing, and admission | Hierarchically allocated or location-dependent addressing (domain names and IP addresses), plus transport-specific authentication and routing |
| [2 · streams](#reliable-streams-with-fips-tcp) | `fips-tcp` | Reliable ordered delivery over FIPS | TCP streams bound to IP endpoints |
| [3a · publish-subscribe](#nostr-pubsub-publish-subscribe) | `nostr-pubsub` | Subscriptions, signed-event exchange, deduplication, source selection, and real-time policy | Central message broker |
| [3b · content](#hashtree-blobs-and-routes) | Hashtree | Hash-addressed files and directories, verification, caching, content routing, apps, releases, history, and Git data | Origin server, CDN, or cloud store as content authority |
| [4a · indexes](#hashtree-indexes-for-large-datasets) | `@hashtree/index` and `@hashtree/collection` | Immutable B-trees, canonical records, derived search roots, collection manifests, and local or federated reads | Central database, search service, or relay index |
| [4b · social context](#social-graph-as-local-policy) | `nostr-social-graph` | Graph traversal, fact events, viewer-local naming, moderation, reputation, and resource-policy inputs | Central profile, ACL, moderation, or reputation database |
| [Settlement plane](#cashu-service-layer) | Cashu service layer | Bounded credit, token transfer, useful-service accounting, and settlement adapters | Credit-card processor or platform balance |
| [5 · applications](#products) | Products | User experience, authorization, durable state, economics, and explicit outbound peers | Single platform as identity, policy, and egress authority |

## 2. Connectivity

### 2.1 FIPS authenticated datagrams

[FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips)
(Free Internetworking Peering System) gives each node a self-generated public-key address and authenticated,
encrypted datagrams. FIPS is not tied to one OSI layer: carrier adapters present
packet delivery, addressing, and MTU to the mesh protocol. An adapter may run
over Ethernet, Bluetooth LE, UDP, TCP, Tor, WebRTC, a relay, or another medium.
Several carriers can participate in
one routed mesh; none is required to be IP-based.

Its identity is the same public-key type and format as a Nostr public key, so
one key can identify both a FIPS node and the author of Nostr events.

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

## 3. Identity, publish-subscribe, and social context

### 3.1 Nostr identity and signed facts

[Nostr](https://github.com/nostr-protocol/nips) keys provide the portable
identity shared by FIPS nodes and application events. Signed events carry
profiles, follows, mutes, messages, capability
adverts, ratings, release roots, and application-defined facts. Signatures bind
an event to its author; each application decides what that author may do.

The [`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph)
identity tools add UUID-based rosters and facts for identities that span several
keys or devices. They support key changes without changing every reference to
the identity.

### 3.2 nostr-pubsub publish-subscribe

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
| Which Ethernet, Bluetooth LE, UDP, WebRTC, or other transport endpoint can I connect FIPS to? | Treat the endpoint and claimed identity as a candidate, then authenticate it with FIPS. |
| Which FIPS identity offers Hashtree, `nostr-pubsub`, or another capability? | Connect to or reuse that peer, then use the advertised interface. |

The DHCP reference applies to one form of endpoint discovery: *which reachable
transport endpoint can I connect FIPS to?* Capability adverts also answer
*which FIPS identity offers this service?* FIPS authenticates each candidate,
and local author, social, capability, and resource policy decides admission.
Relays, indexes, and other pub/sub routes can answer the same request.

### 3.4 Social graph as local policy

[`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph)
turns follows, mutes, signed facts, and ratings into viewer-relative signals.
Each app chooses whose signals to trust, how far they travel, and how to handle
unknown identities. There is no global trust score.

| Input | Local decision |
| --- | --- |
| Follows, social distance, and mutes | Prioritize ordinary Nostr posts from authors near the viewer in the social graph; filter feeds, replies, notifications, profiles, and search results. |
| Machine observations and signed peer or service ratings | Prefer healthy FIPS and pub/sub peers, downrank degraded sources, and preserve exploration space for unknown peers. |
| UUID identity rosters, facts, and attestations | Keep identity consistent across keys and devices; rank contextual names and decide which claims an app accepts. |

The [`nostr-pubsub-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub/crates/nostr-pubsub-social-graph)
adapter uses social distance, nearby mutes, and signed service ratings to allow,
throttle, drop, or prioritize Nostr event authors and pub/sub peers. Nostr VPN
uses this policy for peer and event admission. [FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips)
publishes machine-generated peer ratings and can use selected rating authors to
order candidate peers during discovery.

[Hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree)
uses the graph for crawl scope, relay and storage access, mirror selection, and
profile search. Iris Contacts, Drive, Git, and Social use it to rank profiles,
search results, feeds, and sharing contacts.

These signals can feed resource schedulers. A Hashtree node can serve or
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

The naming architecture combines Hashtree-backed profile search and social
ranking with `nostr-social-memory` UUID identities, petnames, aliases, and
multiple keys.
[Iris Contacts](https://contacts.iris.to/) combines profiles, graph-ranked
search, and its own UUID-backed contacts encoded as fact snapshots; its
[source is on Iris Git](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-contacts).

## 4. Verifiable content and indexes

### 4.1 Hashtree blobs and routes

[Hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree)
stores files as blobs—which can be split into chunks—and directories as manifest
trees. Each blob or manifest is addressed and independently verified by hash.
It presents one operation—fetch a blob by hash—across configured sources.
Anything that implements that operation is a `BlobRoute`.

[Blossom](https://github.com/hzrd149/blossom) provides compatible HTTP storage
for SHA-256-addressed blobs. Related proposals define
[client-side CHK encryption (BUD-15)](https://github.com/hzrd149/blossom/pull/104),
[directory manifests (BUD-16)](https://github.com/hzrd149/blossom/pull/105),
[chunked file and directory fanout manifests (BUD-17)](https://github.com/hzrd149/blossom/pull/106),
and [Hashtree references (BUD-18)](https://github.com/hzrd149/blossom/pull/107).
Together, these BUDs make the encoding canonical: files may be encrypted chunks,
and directories may be manifest trees. Client-side CHK encryption is the default;
each resulting blob or manifest remains content-hash addressed and independently
verifiable. Blossom servers continue to store ordinary blobs.

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

[Iris Sites at apps.iris.to](https://apps.iris.to/) lists and launches these
apps. The catalog aids discovery; signed roots identify publisher versions and
Hashtree hashes verify the app bytes.
[Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-sites).

`hashtree-updater` applies the same model to native releases. A signed root
arrives through `nostr-pubsub`; the app checks the publisher, fetches the
release through its Hashtree routes, verifies the content address, and installs
the matching artifact.

Runtime updates can receive both notice and bytes from stack-native peers, with
relay, Blossom, and HTTPS compatibility routes available.
[`hashtree-updater` source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/rust/crates/hashtree-updater).

## 5. Cashu service layer

[`cashu-service`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service)
provides [Cashu](https://cashu.space/) token transfer and settlement together
with sat-denominated useful-service receipts and bounded peer credit. A node can
accept a peer's credit up to a local limit, then request settlement through an
accepted Cashu mint, Lightning, or another configured method.

Products choose pricing, credit limits, and accepted settlement methods. The
same adapters can account for connectivity, bandwidth, storage, routing, or
other services, while free and reciprocal routes remain available.

## 6. Products

The broader app catalog is [apps.iris.to](https://apps.iris.to/); it lists many
apps beyond the examples below.

| Product | Function | Links |
| --- | --- | --- |
| Iris Chat | Encrypted, local-first messaging without phone-number or email signup | [Product page](https://irischat.org/) · [Web app](https://chat.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-chat) |
| Iris Drive | Offline-first, content-addressed file sync and collaboration across devices, peers, and optional storage providers | [Product page](https://getdrive.iris.to/) · [Web app](https://drive.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-drive-web) |
| Nostr VPN | A private mesh that connects directly when possible, and a public exit-node marketplace when you need an internet route | [Product page](https://nostrvpn.org/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-vpn) |
| Iris Contacts | Public-key profiles, social context, and local UUID-backed contacts without a global account directory | [Web app](https://contacts.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-contacts) |
| Iris Audio | A Hashtree-backed music catalog that demonstrates portable collection and search indexes | [Web app](https://audio.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-audio) |
| Iris Sites | A launcher and isolated browser runtime for web apps published as Hashtree trees | [App catalog](https://apps.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-sites) |
| Iris Git | Git repositories addressed through Nostr and Hashtree instead of one forge account or origin server | [Web app](https://git.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-git) |

## 7. Repository index

| Component | Source |
| --- | --- |
| Iris Stack architecture | [iris-stack](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-stack) · [GitHub mirror](https://github.com/irislib/iris-stack) |
| Nostr specifications | [nostr-protocol/nips](https://github.com/nostr-protocol/nips) |
| FIPS (Rust) | [fips](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips) · [GitHub mirror](https://github.com/mmalmi/fips) |
| FIPS (TypeScript) | [fips-ts](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-ts) · [GitHub mirror](https://github.com/mmalmi/fips-ts) |
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
