# Iris Stack

Iris Stack is a set of permissionless building blocks for identity,
communication, connectivity, storage, social trust, and money. Independent
participants cooperate under the same rules without making a platform account,
domain, server, or payment provider their authority.

- [Developer overview](docs/iris-stack.md)
- [Status, repository boundaries, and test ownership](docs/status-and-testing.md)
- [Integration lab](docs/integration-lab.md)
- [Rendered overview](https://stack.iris.to/)
- [Machine-readable ownership map](stack.json)

## Capability order

The order runs from network-facing substrate toward applications. Identity and
settlement cross every layer.

| Position | Component | Role |
| --- | --- | --- |
| Identity plane | [Nostr](https://github.com/nostr-protocol/nips) | Portable keys, signed events, and claims |
| 1 · datagrams | [FIPS](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips) | Authenticated public-key-addressed links and routing |
| 2 · streams | [`fips-tcp`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/fips-tcp) | Reliable ordered delivery over FIPS |
| 3a · events | [`nostr-pubsub`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-pubsub) | Real-time signed-event subscriptions and distribution |
| 3b · content | [Hashtree](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/hashtree) | Verifiable content, indexes, apps, releases, and Git data |
| 4 · social context | [`nostr-social-graph`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/nostr-social-graph) | Viewer-local naming, content filtering, peer reputation, and resource-policy inputs |
| Settlement plane | [`cashu-service`](https://git.iris.to/#/npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm/cashu-service) | Useful-service accounting and permissionless payment |
| 5 · applications | [Iris products](docs/iris-stack.md#6-products) | User experience, authorization, durable state, and product policy |

The overview explains the layer boundaries, old-stack reference points,
peer discovery, social policy and naming, portable indexes, blob routes, app publishing,
updates, and products. `stack.json` is the canonical compact ownership map.

## Site

The website renders `docs/iris-stack.md` directly.

```sh
pnpm install
pnpm dev
pnpm test
pnpm build
```
