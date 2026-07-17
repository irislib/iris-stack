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

The numbered order below runs from the network-facing substrate toward
applications. Nostr identity and payments cross every layer.

| Position | Component | Role | Replaces |
| --- | --- | --- | --- |
| [Identity](#nostr-identity-and-signed-events) | [Nostr](https://github.com/nostr-protocol/nips) | Portable public-key identity and signed event format | Platform account, email address, phone number, domain name, or TLS certificate as identity |
| [1 · datagrams](#fips-authenticated-datagrams) | [FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips) | Authenticated datagrams addressed by self-generated public keys, encrypted links, carrier adapters, discovery, routing, and admission | Hierarchically allocated or location-dependent addressing (domain names and IP addresses), plus transport-specific authentication and routing |
| [2 · streams](#reliable-streams-with-fips-tcp) | [`fips-tcp`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-tcp) | Reliable ordered delivery over FIPS | TCP streams bound to IP endpoints |
| [3a · publish-subscribe](#nostr-pubsub-publish-subscribe) | [`nostr-pubsub`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub) | Subscriptions, signed-event exchange, deduplication, source selection, and real-time policy | Central message broker |
| [3b · private conversations](#nostr-double-ratchet) | [`nostr-double-ratchet`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-double-ratchet) | End-to-end encrypted 1:1 and group Nostr events, multi-device sessions, and asynchronous delivery | Platform messaging service or always-online encrypted connection |
| [3c · content](#hashtree-blobs-and-routes) | [Hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree) | Hash-addressed files and directories, verification, caching, content routing, apps, releases, history, and Git data | Origin server, CDN, or cloud store as content authority |
| [4a · indexes](#hashtree-indexes-for-large-datasets) | [`@hashtree/index`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-index) and [`@hashtree/collection`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-collection) | Immutable B-trees, canonical records, derived search roots, collection manifests, and local or federated reads | Central database, search service, relay index, or platform API |
| [4b · social context](#social-graph-as-local-policy) | [`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph) | Graph traversal, fact events, viewer-local naming, moderation, reputation, and resource-policy inputs | Central profile, ACL, moderation, or reputation database |
| [Payments](#payments) | [`cashu-service`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service) | Bounded credit, token transfer, useful-service accounting, and settlement adapters | Credit-card processor or platform balance |
| [5 · applications](#products) | Products | User experience, authorization, durable state, economics, and explicit outbound peers | Single platform as identity, policy, and egress authority |

## 2. Identity

### 2.1 Nostr identity and signed events

[Nostr](https://github.com/nostr-protocol/nips) keys provide the portable
identity shared by FIPS nodes and application events. Signed events carry
profiles, follows, mutes, messages, capability
adverts, ratings, release roots, and application-defined facts. Signatures bind
an event to its author; each application decides what that author may do.

### 2.2 Signed fact events

[Fact events](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph/nips/fact-events.md) give application data a reusable subject–predicate–object shape. A
durable subject can represent a person, organization, place, review, or other
entity; predicates such as `name`, `controls`, `same_as`, or `member_of`
describe it; and objects supply names, keys, or related entities. Apps can
update or dispute claims and assemble their own trusted view without defining a
new event type for every data model. The signature proves who made a claim, not
that the claim is universally true.

The [`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph)
identity tools use UUID-based rosters and facts so an identity is not tied to a
single key. If a key is lost or compromised, social-graph attestations can
associate a replacement key with the same UUID, preserving the identity.

| Example app | Usage |
| --- | --- |
| [Iris Chat](https://chat.iris.to/) | Uses Nostr identities and owner-signed AppKeys roster fact snapshots to authorize and synchronize linked devices. |
| [Iris Contacts](https://contacts.iris.to/) | Keeps one UUID subject for a contact while `name`, `controls`, and other signed facts describe names, keys, and relationships that may change. |

## 3. Connectivity

### 3.1 FIPS authenticated datagrams

[FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips)
(Free Internetworking Peering System) gives each node a self-generated public-key address and authenticated,
encrypted datagrams. FIPS is not tied to one OSI layer: carrier adapters present
packet delivery, addressing, and MTU to the mesh protocol. An adapter may run
over Ethernet, Bluetooth LE, UDP, TCP, Tor, WebRTC, a relay, or another medium.
Several carriers can participate in
one routed mesh; none is required to be IP-based.

FIPS uses the same public-key type and format as a Nostr public key, so one key
can identify both a node and an event author.

Applications address a FIPS identity while the node handles peer discovery,
path selection, forwarding, admission, and link health. An IPv6 adapter lets
existing IP software reach the same identities; native applications use FIPS
service datagrams directly.

### 3.2 Reliable streams with fips-tcp

[`fips-tcp`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-tcp)
adds reliable, ordered byte streams above FIPS datagrams. It handles loss,
retransmission, ordering, flow control, and congestion control so applications
do not each invent acknowledgements, retries, and “did you get my message?”
schemes. The remote address remains an authenticated FIPS identity.

The stack uses these streams for Hashtree blob transfers, Iris Chat linked-device
sync, Iris Drive synchronization control messages, and `nostr-pubsub`
inventory/want exchanges.

| Example app | Usage |
| --- | --- |
| [Nostr VPN](https://nostrvpn.org/) | Uses FIPS identities for private mesh peers, allowing carriers to change without changing the identity referenced by routes and access policy. |
| [Iris Chat](https://chat.iris.to/) | Uses `fips-tcp` for reliable, ordered linked-device snapshots and control records. |
| [Iris Drive](https://getdrive.iris.to/) | Uses `fips-tcp` for reliable multi-frame Hashtree transfers and synchronization control messages between authenticated peers, then verifies content by hash above the stream. |

## 4. Publish-subscribe and discovery

### 4.1 nostr-pubsub publish-subscribe

[`nostr-pubsub`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub)
carries ordinary Nostr subscriptions and signed events across local indexes,
FIPS peers, mesh peers, and optional relays. An application subscribes once and
applies local policy when choosing sources or accepting events.

The protocol preserves that model without organizing communication around relay
servers. Peers can exchange and forward subscriptions and events directly,
while standard Nostr relays remain optional routes. A peer only needs a path to
another peer; it does not need to expose a public server, register a domain, or
obtain a TLS certificate. Signatures decentralize authorship, but peer-to-peer
pub/sub is what also decentralizes live delivery.

The [`nostr-pubsub-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub/crates/nostr-pubsub-social-graph)
adapter uses follows, mutes, graph distance, and signed service ratings to admit
incoming events to local storage and fanout and to prefer, throttle, or drop
peer and relay sources. These are viewer-local choices, not network-wide bans.

The same event plane carries social posts and stack coordination events:
peer adverts, machine ratings, Hashtree roots, app updates, repository
announcements, and service offers. Events announce large content by hash;
Hashtree routes carry the bytes.

### 4.2 Signed peer and service discovery

Discovery adverts are candidates, not authority. Signed, expiring transport
adverts say where a claimed FIPS identity may be reachable over Ethernet,
Bluetooth LE, UDP, WebRTC, or another carrier. Capability adverts say what a
FIPS identity offers—such as Hashtree or `nostr-pubsub`—and identify the
interface to use.

The client authenticates the remote identity through FIPS, checks the exact
capability, and applies its own author, social, and resource policy before using
the peer. Seeing an advert alone grants no access or trust.

An existing authenticated FIPS peer is one bootstrap route, not a registry.
Through `nostr-pubsub-fips`, ordinary subscriptions and signed adverts travel
over that connection. Relays, local indexes, and other pub/sub peers can answer
the same query; no source is mandatory.

| Example app | Usage |
| --- | --- |
| [Iris Chat](https://chat.iris.to/) | Uses peer-to-peer pub/sub for live message subscriptions while retaining optional relay routes. |
| [Nostr VPN](https://nostrvpn.org/) | Publishes peer, route, and service announcements; an exit node can advertise both its reachable FIPS identity and its offered service. |

## 5. Private conversations

### 5.1 nostr-double-ratchet

[`nostr-double-ratchet`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-double-ratchet)
provides end-to-end encrypted 1:1 and group conversations over Nostr. Direct
sessions use Double Ratchet, protecting earlier messages if a current key is
compromised and recovering future secrecy after fresh ratchet steps. Groups use
per-sender key chains, and AppKeys authorize multiple devices for one identity.

Ciphertexts are Nostr events, so `nostr-pubsub` peers and ordinary relays can
carry and retain them when participants are not online together. The encrypted
payload can be any app-defined Nostr event—not only a chat line—making the same
primitive useful for private social posts, shared records, and other
group-scoped data.

| Example app | Usage |
| --- | --- |
| [Iris Chat](https://chat.iris.to/) | Uses 1:1 ratchets and group sender keys for encrypted messages across authorized devices, with Nostr routes supporting asynchronous delivery. |

This design is pairwise-ratchet-first: group sender keys are distributed over
authenticated pairwise sessions. [Marmot](https://github.com/marmot-protocol/marmot)
instead uses MLS as a continuous group key-agreement and membership-state
protocol.

## 6. Verifiable content and indexes

### 6.1 Hashtree blobs and routes

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

### 6.2 Hashtree indexes for large datasets

- [`@hashtree/index`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-index)
  stores immutable B-trees for exact, range, prefix, and lightweight text
  queries over a content root.
- [`@hashtree/collection`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-collection)
  adds canonical by-ID records, derived key/search roots, and a manifest.
- An indexer may curate centrally and publish snapshots through a signed
  mutable `npub/tree`; readers can fetch blocks from caches, FIPS peers, or
  compatible stores, verify them, query locally, mirror the published root, or
  derive their own root by adding or removing records while reusing unchanged
  blocks.

Once the root and its blocks are available locally or through mirrors, the
snapshot remains readable even if its maintainer goes offline. Unlike a live
Nostr relay or centralized database API, answering queries does not require the
original indexing service.

This supports reproducible Nostr-relay-like read projections. The publisher
defines the available queries and update cadence; applications can query or
federate several compatible publishers.

### 6.3 Web apps and updates as verified trees

A static web app can be published as a Hashtree directory. Its `nhash`
identifies one immutable version; a signed `npub/tree` name can advance to a
new version without invalidating the old one.

A local app runtime can serve executable sites from separate browser origins
such as `sitename.npub.iris.localhost` or `<nhash>.iris.localhost`, keeping
unrelated apps from sharing cookies, storage, or service workers. Application
sandboxing remains a separate concern.

`hashtree-updater` applies the same model to native releases. A signed root
arrives through `nostr-pubsub`; the app checks the publisher, fetches the
release through its Hashtree routes, verifies the content address, and installs
the matching artifact. Notices and artifacts can come from stack-native peers,
with relay, Blossom, and HTTPS compatibility routes available.

| Example app | Usage |
| --- | --- |
| [Iris Drive](https://getdrive.iris.to/) | Fetches verified files from a local cache, nearby peer, or remote provider, and serves Hashtree apps from isolated local origins. |
| [Iris Audio](https://audio.iris.to/) | Queries shared song, artist, and album collection/search roots directly from the browser. |
| [Iris Sites](https://apps.iris.to/) | Catalogs and launches web apps whose signed roots identify versions and whose Hashtree hashes verify the bytes. |

## 7. Social context and contextual naming

### 7.1 Social graph as local policy

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
throttle, drop, or prioritize Nostr event authors and pub/sub peers.
[FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips)
publishes machine-generated peer ratings and can use selected rating authors to
order candidate peers during discovery.

[Hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree)
uses the graph for crawl scope, relay and storage access, mirror selection, and
profile search.

These signals can guide resource scheduling: Hashtree can prioritize socially
close or reputable peers, while FIPS can reserve connection slots or bandwidth.
Each node controls its schedule and can reserve capacity for unfamiliar peers.

### 7.2 Human names without a global namespace

Cryptographic keys are secure addresses but poor human names. Naming is a
signed search problem:

- Nostr profiles propose names for public keys.
- Fact events attach `name` claims to durable UUIDs and `controls` claims to
  keys, preserving identity while keys or names change.
- Hashtree stores provide exact tag lookup; `@hashtree/index` or another
  database can add tokenized search.
- Search returns candidates. Viewer-local follows, social distance, explicit
  trust, and application policy rank them.

Names are signed claims, not registrations. The same string can identify
several candidates. Each viewer resolves it through accepted authors, social
context, and private petnames while the key or UUID remains stable.

This follows the petname approach described in
[Zooko's original naming essay](https://www.cs.princeton.edu/courses/archive/spr17/cos518/papers/zooko-triangle.pdf)
and [An Introduction to Petname Systems](https://www.skyhunter.com/marcs/petnames/IntroPetNames.html):
published nicknames aid discovery; a name becomes a petname only when a viewer
adopts a private, unambiguous mapping to a secure identity.

The naming architecture combines Hashtree-backed profile search and social
ranking with `nostr-social-memory` UUID identities, petnames, aliases, and
multiple keys.

| Example app | Usage |
| --- | --- |
| [Nostr VPN](https://nostrvpn.org/) | Uses graph distance, mutes, and signed service ratings for peer and event admission. |
| [Iris Contacts](https://contacts.iris.to/) | Combines profiles, graph-ranked search, contextual names, and UUID-backed contacts encoded as fact snapshots. |
| [Iris Drive](https://getdrive.iris.to/) | Uses social context to rank profiles, search results, sharing contacts, and candidate providers. |

## 8. Payments

[`cashu-service`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service)
provides [Cashu](https://cashu.space/) token transfer and settlement together
with sat-denominated useful-service receipts and bounded peer credit. A node can
accept a peer's credit up to a local limit, then request settlement through an
accepted Cashu mint, Lightning, or another configured method.

Products choose pricing, credit limits, and settlement methods. The same
adapters can meter connectivity, bandwidth, storage, or routing, while free and
reciprocal routes remain available.

| Example app | Usage |
| --- | --- |
| [Nostr VPN](https://nostrvpn.org/) | Lets an exit node charge for forwarded traffic and settle the balance with Cashu. |

## 9. Products

More apps are listed at [apps.iris.to](https://apps.iris.to/).

| Product | Function | Links |
| --- | --- | --- |
| Iris Chat | Encrypted, local-first messaging without phone-number or email signup | [Product page](https://irischat.org/) · [Web app](https://chat.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-chat) |
| Iris Drive | Offline-first, content-addressed file sync and collaboration across devices, peers, and optional storage providers | [Product page](https://getdrive.iris.to/) · [Web app](https://drive.iris.to/) · [Native source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-drive) · [Web source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-drive-web) |
| Nostr VPN | A private mesh that connects directly when possible, and a public exit-node marketplace when you need an internet route | [Product page](https://nostrvpn.org/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-vpn) |
| Iris Contacts | Public-key profiles, social context, and local UUID-backed contacts without a global account directory | [Web app](https://contacts.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-contacts) |
| Iris Audio | A Hashtree-backed music catalog that demonstrates portable collection and search indexes | [Web app](https://audio.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-audio) |
| Iris Sites | A launcher and isolated browser runtime for web apps published as Hashtree trees | [App catalog](https://apps.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-sites) |
| Iris Git | Git repositories addressed through Nostr and Hashtree; `git-remote-htree` gives command-line Git fetch and push over `htree://` remotes | [Web app](https://git.iris.to/) · [Source](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-git) · [Remote helper](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/rust/crates/git-remote-htree) |

### 9.1 Product composition

Current composition names integrations already present in product source and
ordinary product paths. Optional or in-progress paths are identified separately
and are not presented as shipped defaults.

| Product | Current composition | Optional or in progress |
| --- | --- | --- |
| Iris Chat | [Nostr](#nostr-identity-and-signed-events) events, [`nostr-pubsub`](#nostr-pubsub-publish-subscribe) routing, [`nostr-double-ratchet`](#nostr-double-ratchet) messages, [FIPS](#fips-authenticated-datagrams) live links, [`fips-tcp`](#reliable-streams-with-fips-tcp) linked-device sync, and [Hashtree](#hashtree-blobs-and-routes) attachments | Ordinary Nostr relay routes remain compatible |
| Iris Drive | [Hashtree](#hashtree-blobs-and-routes) files, directories, adaptive blob routes and pooled storage; [FIPS](#fips-authenticated-datagrams) authenticated peers and relay/WebRTC bootstrap; [`fips-tcp`](#reliable-streams-with-fips-tcp) blob streams; and the native `iris.localhost` resolver | Paid storage routes remain optional |
| Nostr VPN | [FIPS](#fips-authenticated-datagrams) private and routed mesh, [`nostr-pubsub`](#nostr-pubsub-publish-subscribe) control events, and [social graph](#social-graph-as-local-policy) peer policy | [Cashu](#payments) settlement for paid exits is optional |
| Iris Contacts | [Nostr](#nostr-identity-and-signed-events) profiles, [social graph](#social-graph-as-local-policy), and UUID-backed fact snapshots | — |
| Iris Audio | [Hashtree](#hashtree-blobs-and-routes) media, [search indexes and collections](#hashtree-indexes-for-large-datasets), and Nostr mutable-root announcements | FIPS content routes are optional |
| Iris Sites | [Hashtree](#hashtree-blobs-and-routes) app trees, Nostr mutable roots, isolated web origins, and the Iris Drive resolver | — |
| Iris Git | Nostr repository roots, [Hashtree](#hashtree-blobs-and-routes) Git data, [social graph](#social-graph-as-local-policy) context, and [`git-remote-htree`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/rust/crates/git-remote-htree) | — |

## 10. Repository index

| Component | Source |
| --- | --- |
| Iris Stack architecture | [iris-stack](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-stack) · [GitHub mirror](https://github.com/irislib/iris-stack) |
| Nostr specifications | [nostr-protocol/nips](https://github.com/nostr-protocol/nips) |
| FIPS (Rust) | [fips](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips) · [GitHub mirror](https://github.com/mmalmi/fips) |
| FIPS (TypeScript) | [fips-ts](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-ts) · [GitHub mirror](https://github.com/mmalmi/fips-ts) |
| Reliable FIPS streams | [fips-tcp](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-tcp) · [GitHub mirror](https://github.com/mmalmi/fips-tcp) |
| Decentralized pub/sub | [nostr-pubsub](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub) · [GitHub mirror](https://github.com/mmalmi/nostr-pubsub) |
| Content-addressed storage and routing | [hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree) · [GitHub mirror](https://github.com/mmalmi/hashtree) |
| Git remote helper for `htree://` URLs | [`git-remote-htree`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/rust/crates/git-remote-htree) · [GitHub mirror](https://github.com/mmalmi/hashtree/tree/master/rust/crates/git-remote-htree) |
| Content-addressed search indexes | [`@hashtree/index`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-index) · [GitHub mirror](https://github.com/mmalmi/hashtree/tree/master/ts/packages/hashtree-index) |
| Content-addressed collections | [`@hashtree/collection`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/ts/packages/hashtree-collection) · [GitHub mirror](https://github.com/mmalmi/hashtree/tree/master/ts/packages/hashtree-collection) |
| App updates over Hashtree | [`hashtree-updater`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree/rust/crates/hashtree-updater) · [GitHub mirror](https://github.com/mmalmi/hashtree/tree/master/rust/crates/hashtree-updater) |
| Social graph and fact events | [nostr-social-graph](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph) · [GitHub mirror](https://github.com/mmalmi/nostr-social-graph) |
| Cashu service primitives | [cashu-service](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service) |
| Private-message ratchet | [nostr-double-ratchet](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-double-ratchet) · [GitHub mirror](https://github.com/irislib/nostr-double-ratchet) |
| Shared web integration | [iris-kit](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-kit) · [GitHub mirror](https://github.com/mmalmi/iris-kit) |
| Iris Stack architecture and integration lab | [iris-stack](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/iris-stack) · [GitHub mirror](https://github.com/irislib/iris-stack) |
