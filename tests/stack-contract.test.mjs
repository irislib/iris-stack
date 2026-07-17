import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';

const root = resolve(import.meta.dirname, '..');
const stack = JSON.parse(readFileSync(resolve(root, 'stack.json'), 'utf8'));
const guide = readFileSync(resolve(root, 'docs/iris-stack.md'), 'utf8');
const readme = readFileSync(resolve(root, 'README.md'), 'utf8');
const siteApp = readFileSync(resolve(root, 'site/src/App.svelte'), 'utf8');
const siteIndex = readFileSync(resolve(root, 'site/index.html'), 'utf8');
const cargoManifest = readFileSync(resolve(root, 'Cargo.toml'), 'utf8');
const packageMetadata = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8'));
const nativeWorkflow = readFileSync(resolve(root, '.github/workflows/ci.yml'), 'utf8');
const productWorkflow = readFileSync(resolve(root, '.github/workflows/product-lab.yml'), 'utf8');
const vpnLab = readFileSync(resolve(root, 'scripts/vpn-product-lab.sh'), 'utf8');
const vpnWorkflow = readFileSync(resolve(root, '.github/workflows/vpn-product-lab.yml'), 'utf8');

test('the machine-readable stack includes the documented capability layers', () => {
  const orderedIds = [
    'nostr',
    'fips',
    'fips-tcp',
    'nostr-pubsub',
    'nostr-double-ratchet',
    'hashtree',
    'nostr-social-graph',
    'cashu-service',
  ];
  const componentIds = stack.components.map(({ id }) => id);
  const positions = orderedIds.map((id) => componentIds.indexOf(id));
  assert(positions.every((position) => position >= 0), 'expected every documented capability');
  assert(positions.every((position, index) => index === 0 || position > positions[index - 1]));
});

test('the public artifact keeps portable metadata and project ownership explicit', () => {
  assert.match(siteIndex, /<title>Iris Stack<\/title>/);
  assert.match(siteIndex, /<meta property="og:image" content="https:\/\/stack\.iris\.to\/social-preview\.png" \/>/);
  assert.match(siteIndex, /<meta name="twitter:card" content="summary" \/>/);
  assert(existsSync(resolve(root, 'site/public/social-preview.png')));
  assert.equal('license' in packageMetadata, false);
  assert.doesNotMatch(cargoManifest, /^license\s*=/m);
  assert.equal(existsSync(resolve(root, 'LICENSE')), false);
});

test('machine-readable product composition matches the exercised integrations', () => {
  const lab = stack.components.find(({ id }) => id === 'iris-stack-lab');
  const products = new Map(stack.products.map((product) => [product.id, product]));

  assert.deepEqual(lab.depends_on, ['fips', 'fips-tcp', 'hashtree', 'cashu-service']);
  for (const capability of ['fips-tcp', 'hashtree']) {
    assert(products.get('iris-chat').intended_capabilities.includes(capability));
  }
  for (const capability of ['nostr-pubsub', 'nostr-social-graph']) {
    assert(products.get('nostr-vpn').intended_capabilities.includes(capability));
  }
});

test('the public guide links products without leaking private workspace details', () => {
  for (const url of [
    'https://irischat.org/',
    'https://chat.iris.to/',
    'https://getdrive.iris.to/',
    'https://drive.iris.to/',
    'https://nostrvpn.org/',
    'https://git.iris.to/',
    'https://contacts.iris.to/',
    'https://audio.iris.to/',
    'https://apps.iris.to/',
  ]) {
    assert(guide.includes(url), `expected public guide link ${url}`);
  }
  assert.doesNotMatch(`${readme}\n${guide}\n${siteApp}`, /\/Users\/|~\/src\/|mission-control|\.keys\//i);
});

test('the VPN product gate delegates to one pinned owner harness', () => {
  const revision = '0a73e41794653202e95f72ff86d75e29c6339449';

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

test('push and pull-request CI covers native and product composition', () => {
  assert.match(nativeWorkflow, /^  push:/m);
  assert.match(nativeWorkflow, /^  pull_request:/m);
  assert.match(nativeWorkflow, /cargo test --locked --all-targets/);
  assert.match(productWorkflow, /^  push:\n    paths:/m);
  assert.match(productWorkflow, /^  pull_request:\n    paths:/m);
  assert.match(productWorkflow, /^  chat-drive-hashtree:\n    runs-on:/m);
});
