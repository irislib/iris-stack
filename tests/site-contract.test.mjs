import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const app = readFileSync(resolve(root, 'site/src/App.svelte'), 'utf8');
const styles = readFileSync(resolve(root, 'site/src/styles.css'), 'utf8');
const icon = readFileSync(resolve(root, 'site/public/favicon.svg'), 'utf8');
const readme = readFileSync(resolve(root, 'README.md'), 'utf8');
const guide = readFileSync(resolve(root, 'docs/iris-stack.md'), 'utf8');
const stack = JSON.parse(readFileSync(resolve(root, 'stack.json'), 'utf8'));
const nativeWorkflow = readFileSync(resolve(root, '.github/workflows/ci.yml'), 'utf8');
const productWorkflow = readFileSync(resolve(root, '.github/workflows/product-lab.yml'), 'utf8');
const vpnLab = readFileSync(resolve(root, 'scripts/vpn-product-lab.sh'), 'utf8');
const vpnWorkflow = readFileSync(resolve(root, '.github/workflows/vpn-product-lab.yml'), 'utf8');

const productUrls = [
  'https://irischat.org/',
  'https://chat.iris.to/',
  'https://getdrive.iris.to/',
  'https://drive.iris.to/',
  'https://nostrvpn.org/',
  'https://git.iris.to/',
  'https://contacts.iris.to/',
  'https://audio.iris.to/',
  'https://apps.iris.to/',
];

test('the public site connects the architecture to live products', () => {
  for (const url of productUrls) {
    assert(guide.includes(url), `expected the public guide to link ${url}`);
  }
});

test('the site directly renders the repository-owned Markdown guide', () => {
  assert.match(app, /iris-stack\.md\?raw/);
  assert.match(app, /marked\.parse/);
  assert.doesNotMatch(app, /mermaid|Renderer|flowchart/i);
  assert.match(app, /chapter\.children/);
  assert.match(styles, /Source Serif 4/);
  assert.match(styles, /\.doc-layout aside nav \{[^}]*Source Serif 4/s);
  assert(existsSync(resolve(root, 'site/src/assets/source-serif-4-variable-roman.woff2')));
  assert(existsSync(resolve(root, 'site/public/fonts/source-serif-4-license.txt')));
  assert.doesNotMatch(app, /hero|product-card|button-primary/);
  assert(readme.includes('docs/iris-stack.md'));
  assert.match(icon, /aria-label="Iris Stack"/);
});

test('the public guide keeps architecture prose and private operations out', () => {
  assert.doesNotMatch(guide, /```mermaid|^flowchart /m);
  assert(guide.indexOf('## 1. Capability layers') < guide.indexOf('### 1.1 Old-stack reference points'));
  assert.match(guide, /## Vision/);
  assert.match(guide, /community-built networks/);
  assert.match(guide, /Cashu, Lightning, and Bitcoin/);
  assert.match(guide, /humans, personal agents, and services/);
  assert.match(guide, /Decentralized compute is another possible route type/);
  assert.match(guide, /\[FIPS\]\(https:\/\/git\.iris\.to\//);
  assert.match(guide, /same public-key type and format as a Nostr public key/);
  assert.match(guide, /\[`nostr-pubsub`\]\(https:\/\/git\.iris\.to\//);
  assert.match(guide, /DHCP reference applies to endpoint discovery/);
  assert.match(guide, /nostr-pubsub-fips/);
  assert.match(guide, /Social graph as local policy/);
  assert.match(guide, /nostr-pubsub-social-graph/);
  assert.match(guide, /machine-generated peer ratings/);
  assert.match(guide, /Hashtree node can serve or\s+fetch.*FIPS host can reserve/s);
  assert.match(guide, /no global trust score/);
  assert.match(guide, /Zooko's original naming essay/);
  assert.match(guide, /Introduction to Petname Systems/);
  assert.match(guide, /Hashtree indexes for large datasets/);
  assert.match(guide, /Web apps and updates as verified trees/);
  assert.match(guide, /hashtree-updater/);
  assert.match(guide, /Hashtree routes/);
  assert.match(guide, /public exit-node marketplace/);
  assert.match(guide, /The broader app catalog is \[sites\.iris\.to\]\(https:\/\/sites\.iris\.to\//);
  assert.match(guide, /sites\.iris\.to.*redirects there/s);
  assert.doesNotMatch(guide, /These comparisons are orientation|Composition map|Open boundaries|Process composition|Same-host application topology/);
  assert.doesNotMatch(guide, /\/Users\/|~\/src\/|wiki\/projects|release candidate|local master/i);
  assert.doesNotMatch(
    `${readme}\n${guide}`,
    /needless serialization|wire codec|random nonce|127\.0\.0\.1|Noise IK|encrypted FSP|BlobRequest|BlobReply|forced death|released-artifact/i,
  );
});

test('the machine-readable stack keeps the capabilities used by the visual map', () => {
  const componentIds = new Set(stack.components.map(({ id }) => id));
  for (const id of ['nostr', 'fips', 'fips-tcp', 'nostr-pubsub', 'hashtree', 'nostr-social-graph', 'cashu-service']) {
    assert(componentIds.has(id), `expected stack.json component ${id}`);
  }
});

test('the VPN product gate delegates to one pinned owner harness on bounded triggers', () => {
  const revision = '4c43cc5761d67e5dc1a9a4de30c829ae45dc37f3';

  assert(vpnLab.includes(revision));
  assert(vpnWorkflow.includes(revision));
  assert.match(vpnLab, /IRIS_STACK_NVPN_REV/);
  assert.match(vpnLab, /IRIS_STACK_NVPN_GIT_URL/);
  assert.match(vpnLab, /scripts\/e2e-connect-docker\.sh/);
  assert.doesNotMatch(vpnLab, /\bnvpn\s+(?:set|connect|pubsub)\b/);
  assert.match(vpnWorkflow, /^  workflow_call:/m);
  assert.match(vpnWorkflow, /^  workflow_dispatch:/m);
  assert.match(vpnWorkflow, /^  push:\n    paths:/m);
  assert.match(vpnWorkflow, /^  pull_request:\n    paths:/m);
});

test('push and pull-request CI covers native and released-product composition', () => {
  assert.match(nativeWorkflow, /^  push:/m);
  assert.match(nativeWorkflow, /^  pull_request:/m);
  assert.match(nativeWorkflow, /cargo test --locked --all-targets/);
  assert.match(productWorkflow, /^  push:\n    paths:/m);
  assert.match(productWorkflow, /^  pull_request:\n    paths:/m);
  assert.match(productWorkflow, /^  chat-drive-hashtree:\n    runs-on:/m);
  assert.doesNotMatch(productWorkflow, /github\.event_name.*workflow_(?:call|dispatch)/);
});
