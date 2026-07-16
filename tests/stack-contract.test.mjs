import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';

const root = resolve(import.meta.dirname, '..');
const stack = JSON.parse(readFileSync(resolve(root, 'stack.json'), 'utf8'));
const nativeWorkflow = readFileSync(resolve(root, '.github/workflows/ci.yml'), 'utf8');
const productWorkflow = readFileSync(resolve(root, '.github/workflows/product-lab.yml'), 'utf8');
const vpnLab = readFileSync(resolve(root, 'scripts/vpn-product-lab.sh'), 'utf8');
const vpnWorkflow = readFileSync(resolve(root, '.github/workflows/vpn-product-lab.yml'), 'utf8');

test('the machine-readable stack includes the documented capability layers', () => {
  const componentIds = new Set(stack.components.map(({ id }) => id));
  for (const id of [
    'nostr',
    'fips',
    'fips-tcp',
    'nostr-pubsub',
    'hashtree',
    'nostr-social-graph',
    'cashu-service',
  ]) {
    assert(componentIds.has(id), `expected stack.json component ${id}`);
  }
});

test('the VPN product gate delegates to one pinned owner harness', () => {
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

test('push and pull-request CI covers native and product composition', () => {
  assert.match(nativeWorkflow, /^  push:/m);
  assert.match(nativeWorkflow, /^  pull_request:/m);
  assert.match(nativeWorkflow, /cargo test --locked --all-targets/);
  assert.match(productWorkflow, /^  push:\n    paths:/m);
  assert.match(productWorkflow, /^  pull_request:\n    paths:/m);
  assert.match(productWorkflow, /^  chat-drive-hashtree:\n    runs-on:/m);
});
