import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const app = readFileSync(resolve(root, 'site/src/App.svelte'), 'utf8');
const index = readFileSync(resolve(root, 'site/index.html'), 'utf8');
const styles = readFileSync(resolve(root, 'site/src/styles.css'), 'utf8');
const icon = readFileSync(resolve(root, 'site/public/favicon.svg'), 'utf8');
const readme = readFileSync(resolve(root, 'README.md'), 'utf8');
const guide = readFileSync(resolve(root, 'docs/iris-stack.md'), 'utf8');
const statusGuide = readFileSync(resolve(root, 'docs/status-and-testing.md'), 'utf8');
const stack = JSON.parse(readFileSync(resolve(root, 'stack.json'), 'utf8'));
const packageMetadata = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8'));
const cargoManifest = readFileSync(resolve(root, 'Cargo.toml'), 'utf8');

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
  assert.match(guide, /^## A Freedom Tech Toolkit$/m);
  assert.match(guide, /<figure class="app-catalog">/);
  assert.match(guide, /src="\.\/apps-iris-to\.png"[^>]*width="960"[^>]*height="794"/);
  assert.match(guide, /cache their files for offline launch/);
  assert.match(guide, /features that depend on peers or external services still require connectivity/);
  assert.match(guide, /local <code>iris\.localhost<\/code> resolver/);
  assert.match(guide, /href="#products">Iris Stack products<\/a>/);
  assert.match(guide, /Iris Drive native app/);
  assert.match(guide, /FIPS \(TypeScript\).*\[fips-ts\]\(https:\/\/git\.iris\.to\/#\/[^)]+\/fips-ts\).*\[GitHub mirror\]\(https:\/\/github\.com\/mmalmi\/fips-ts\)/);
  assert.doesNotMatch(guide, /used offline/);
  assert(existsSync(resolve(root, 'site/public/apps-iris-to.png')));
  for (const url of productUrls) {
    assert(guide.includes(url), `expected the public guide to link ${url}`);
  }
});

test('the capability table navigates within the guide', () => {
  assert.match(guide, /^\| Position \| Component \| Role \| Replaces \|$/m);
  assert.match(guide, /^\| Product \| Function \| Links \|$/m);
  assert.equal((guide.match(/^\| Example app \| Usage \|$/gm) ?? []).length, 6);
  for (const chapter of [
    '#nostr-identity-and-signed-events',
    '#fips-authenticated-datagrams',
    '#reliable-streams-with-fips-tcp',
    '#nostr-pubsub-publish-subscribe',
    '#hashtree-blobs-and-routes',
    '#hashtree-indexes-for-large-datasets',
    '#social-graph-as-local-policy',
    '#payments',
    '#products',
  ]) {
    assert(guide.includes(`](${chapter}) |`), `expected a first-column capability-table link to ${chapter}`);
  }
  assert.match(guide, /Authenticated datagrams addressed by self-generated public keys/);
  assert.match(guide, /Hierarchically allocated or location-dependent addressing \(domain names and IP addresses\), plus transport-specific authentication and routing/);
  assert.match(guide, /Platform account, email address, phone number, domain name, or TLS certificate as identity/);
  assert.match(guide, /subject–predicate–object shape/);
  assert.match(guide, /signature proves who made a claim, not\s+that the claim is universally true/);
  assert.match(guide, /If a key is lost or compromised, social-graph attestations can\s+associate a replacement key with the same UUID, preserving the identity/);
  assert.match(guide, /Iris Chat.*owner-signed AppKeys roster fact snapshots to authorize and synchronize linked devices/s);
  assert.match(guide, /Iris Contacts.*Keeps one UUID subject/s);
  assert.match(guide, /Nostr VPN.*Uses FIPS identities for private mesh peers/s);
  assert.match(guide, /Iris Drive.*Uses `fips-tcp` for reliable multi-frame Hashtree transfers/s);
  assert.match(guide, /Origin server, CDN, or cloud store as content authority/);
  assert.match(guide, /Hash-addressed files and directories/);
  assert.match(guide, /files as blobs and directories as trees/);
  assert.match(guide, /\[Blossom\]\(https:\/\/github\.com\/hzrd149\/blossom\)/);
  for (const pull of [104, 105, 106, 107]) {
    assert(guide.includes(`https://github.com/hzrd149/blossom/pull/${pull}`));
  }
});

test('the guide body follows the capability order', () => {
  const orderedHeadings = [
    '## 2. Identity',
    '## 3. Connectivity',
    '## 4. Publish-subscribe and discovery',
    '## 5. Verifiable content and indexes',
    '## 6. Social context and contextual naming',
    '## 7. Payments',
    '## 8. Products',
    '## 9. Repository index',
  ];
  const positions = orderedHeadings.map((heading) => guide.indexOf(heading));
  assert(positions.every((position) => position >= 0), 'expected every capability-ordered guide chapter');
  assert(positions.every((position, index) => index === 0 || position > positions[index - 1]), 'expected guide chapters in capability order');
});

test('repository ownership follows the capability order before support components', () => {
  const organization = statusGuide.slice(
    statusGuide.indexOf('## Repository organization'),
    statusGuide.indexOf('## Test ownership'),
  );
  const orderedRows = [
    '| `nostr-protocol/nips` |',
    '| `fips` / `fips-ts` |',
    '| `fips-tcp` |',
    '| `nostr-pubsub` |',
    '| `hashtree` |',
    '| `@hashtree/index` / `@hashtree/collection` |',
    '| `nostr-social-graph` |',
    '| `cashu-service` |',
    '| Product repositories |',
    '| `nostr-double-ratchet` |',
    '| `iris-kit` |',
    '| `iris-stack` |',
  ];
  const positions = orderedRows.map((row) => organization.indexOf(row));
  assert(positions.every((position) => position >= 0), 'expected every capability and support owner');
  assert(positions.every((position, index) => index === 0 || position > positions[index - 1]), 'expected repository owners in capability order');
});

test('the site directly renders the repository-owned Markdown guide', () => {
  assert.equal('license' in packageMetadata, false);
  assert.doesNotMatch(cargoManifest, /^license\s*=/m);
  assert.equal(existsSync(resolve(root, 'LICENSE')), false);
  assert.match(index, /<title>Iris Stack: A Freedom Tech Toolkit<\/title>/);
  assert.match(index, /<meta property="og:title" content="Iris Stack: A Freedom Tech Toolkit" \/>/);
  assert.match(index, /publish-subscribe communication/);
  assert.doesNotMatch(index, /real-time events/);
  assert.match(app, /iris-stack\.md\?raw/);
  assert.match(app, /marked\.parse/);
  assert.doesNotMatch(app, /mermaid|Renderer|flowchart/i);
  assert.match(app, /chapter\.children/);
  assert.match(app, /class="toc-toggle"/);
  assert.match(app, /aria-controls="table-of-contents"/);
  assert.match(app, /aria-current=\{activeTocId === chapter\.id/);
  assert.match(app, /updateActiveToc/);
  assert.match(app, /activeHeadingOffset = 24/);
  assert.match(app, /scrollPositionTolerance = 1/);
  assert.match(app, /selectToc/);
  assert.doesNotMatch(app, /viewportCenter/);
  assert.match(app, /dataset\.label/);
  assert.match(app, /class="title-icon"/);
  assert.match(app, /<main id="top"/);
  assert.doesNotMatch(app, /<header|class="topbar"|aria-label="Document links"/);
  assert.match(app, /<footer[^>]*>[\s\S]*href=\{sourceUrl\}[\s\S]*Source ↗[\s\S]*<\/footer>/);
  assert.doesNotMatch(app, /0BSD/);
  assert.doesNotMatch(app, /<span id="top"><\/span>/);
  assert.match(app, /heading\.label !== 'A Freedom Tech Toolkit'/);
  assert.match(styles, /Source Serif 4/);
  assert.match(styles, /body \{[^}]*font-family: "Source Serif 4"/s);
  assert.doesNotMatch(styles, /\.app-catalog figcaption \{[^}]*font-family|\.markdown table \{[^}]*font-family/s);
  assert.match(styles, /\.markdown table \{[^}]*display: table;[^}]*width: 100%;/s);
  assert.match(styles, /\.markdown > h1:first-child \+ h2/);
  assert.match(styles, /\.title-icon/);
  assert.match(styles, /@media \(max-width: 800px\)[\s\S]*?\.doc-layout aside nav > ol\.toc-open \{\s*display: block;/);
  assert.match(styles, /\.toc-toggle/);
  assert.match(styles, /\.doc-layout aside a\.active/);
  assert.match(styles, /\.doc-layout aside a\.active::before/);
  assert.doesNotMatch(styles, /\.doc-layout aside a\.active \{[^}]*font-weight/s);
  assert.match(styles, /\.doc-layout aside a\.active \{[^}]*color: var\(--text\)/s);
  assert.doesNotMatch(styles, /\.topbar|\.site-name/);
  assert.match(styles, /footer a/);
  assert.match(styles, /\.doc-layout aside li ol a \{[^}]*var\(--muted\)/s);
  assert.match(styles, /\.table-cell-label/);
  assert.match(styles, /\.doc-layout aside nav \{[^}]*Source Serif 4/s);
  assert(existsSync(resolve(root, 'site/src/assets/source-serif-4-variable-roman.woff2')));
  assert(existsSync(resolve(root, 'site/public/fonts/source-serif-4-license.txt')));
  assert.doesNotMatch(app, /hero|product-card|button-primary/);
  assert(readme.includes('docs/iris-stack.md'));
  assert.match(readme, /^## Documentation$/m);
  assert.doesNotMatch(readme, /^## Capability order$|^\| Position \| Component \| Role \|/m);
  assert.match(icon, /aria-label="Iris Stack"/);
});

test('the public guide keeps architecture prose and private operations out', () => {
  assert.doesNotMatch(guide, /```mermaid|^flowchart /m);
  assert.doesNotMatch(guide, /Old-stack reference points|Familiar authority/);
  assert.doesNotMatch(guide, /## Vision/);
  assert.match(guide, /Free Internetworking\s+Peering System/);
  assert.match(guide, /FIPS connectivity layer can\s+use carrier adapters at multiple layers: Ethernet, Bluetooth LE, UDP, TCP,\s+WebRTC, Tor, or relays/);
  assert.doesNotMatch(guide, /BLE\/L2CAP|UDP\/IP|TCP\/IP/);
  assert.match(guide, /Nostr provides\s+portable signed identity\s+and publish-subscribe messaging; Hashtree verifies\s+content by hash/);
  assert.match(guide, /\[3a · publish-subscribe\]\(#nostr-pubsub-publish-subscribe\)/);
  assert.match(guide, /Hashtree\s+verifies\s+content by hash; viewer-local\s+social graphs supply\s+trust policy; and Cashu\s+provides credit and settlement/);
  assert.match(guide, /FIPS is not tied to one OSI layer/);
  assert.match(guide, /carrier adapters present\s+packet delivery, addressing, and MTU/);
  assert.doesNotMatch(guide, /lower-level links|physical layer/);
  assert.match(guide, /same public-key type and format as a Nostr public key/);
  assert.match(guide, /\[`nostr-pubsub`\]\(https:\/\/git\.iris\.to\//);
  assert.match(guide, /Peers can exchange and forward subscriptions and events directly/);
  assert.match(guide, /Signatures decentralize\s+authorship, but peer-to-peer\s+pub\/sub is what also decentralizes live delivery/);
  assert.match(guide, /admit\s+incoming events to local storage and fanout/);
  assert.match(guide, /prefer, throttle, or drop\s+peer and relay sources/);
  assert.match(guide, /Iris Chat.*Uses peer-to-peer pub\/sub for live message subscriptions/s);
  assert.match(guide, /Discovery adverts are candidates, not authority/);
  assert.match(guide, /existing authenticated FIPS peer is one bootstrap route, not a registry/);
  assert.match(guide, /nostr-pubsub-fips/);
  assert.match(guide, /Social graph as local policy/);
  assert.match(guide, /nostr-pubsub-social-graph/);
  assert.match(guide, /machine-generated peer ratings/);
  assert.match(guide, /Hashtree can prioritize socially\s+close or reputable peers, while FIPS can reserve connection slots or bandwidth/);
  assert.match(guide, /no global trust score/);
  assert.match(guide, /Zooko's original naming essay/);
  assert.match(guide, /Introduction to Petname Systems/);
  assert.match(guide, /Hashtree indexes for large datasets/);
  assert.match(guide, /Web apps and updates as verified trees/);
  assert.match(guide, /hashtree-updater/);
  assert.match(guide, /Hashtree routes/);
  assert.match(guide, /\[Cashu\]\(https:\/\/cashu\.space\/\) token transfer/);
  assert.match(guide, /^## 7\. Payments$/m);
  assert.doesNotMatch(guide, /^## 7\. (?:Settlement|Cashu service layer)$|^### 7\.1 /m);
  assert.doesNotMatch(guide, /Identity plane|Payment plane|Settlement plane/);
  assert.match(guide, /Nostr VPN.*Lets an exit node charge for forwarded traffic/s);
  assert.match(guide, /public exit-node marketplace/);
  assert.match(guide, /More apps are listed at \[apps\.iris\.to\]\(https:\/\/apps\.iris\.to\//);
  assert.doesNotMatch(guide, /sites\.iris\.to/);
  assert.doesNotMatch(guide, /status-and-testing|implementation status|available today|remains to be integrated|as of \d{4}-\d{2}-\d{2}/i);
  assert.doesNotMatch(guide, /These comparisons are orientation|Composition map|Open boundaries|Process composition|Same-host application topology/);
  assert.doesNotMatch(guide, /\/Users\/|~\/src\/|wiki\/projects|release candidate|local master/i);
  assert.doesNotMatch(
    `${readme}\n${guide}`,
    /needless serialization|wire codec|random nonce|127\.0\.0\.1|Noise IK|encrypted FSP|BlobRequest|BlobReply|forced death|released-artifact/i,
  );
});

test('the machine-readable stack keeps the capabilities used by the visual map', () => {
  const orderedIds = ['nostr', 'fips', 'fips-tcp', 'nostr-pubsub', 'hashtree', 'nostr-social-graph', 'cashu-service'];
  const componentIds = stack.components.map(({ id }) => id);
  for (const id of orderedIds) {
    assert(componentIds.includes(id), `expected stack.json component ${id}`);
  }
  const positions = orderedIds.map((id) => componentIds.indexOf(id));
  assert(positions.every((position, index) => index === 0 || position > positions[index - 1]), 'expected stack.json components in capability order');
  assert(stack.components.find(({ id }) => id === 'hashtree').owns.includes('content-addressed B-tree and collection indexes'));
});
