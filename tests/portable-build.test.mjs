import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const dist = resolve(root, 'dist');
const html = readFileSync(resolve(dist, 'index.html'), 'utf8');

test('the production build is portable below an htree path', () => {
  assert(!html.includes('src="/assets/'), 'script asset paths must be relative');
  assert(!html.includes('href="/assets/'), 'stylesheet asset paths must be relative');
  assert(!html.includes('crossorigin'), 'cross-origin module hints break portable delivery');
  assert(!html.includes('modulepreload'), 'module preload hints break portable delivery');
});

test('every local script and stylesheet referenced by index.html exists', () => {
  const references = [...html.matchAll(/(?:src|href)="(\.\/[^"?#]+)"/g)].map((match) => match[1]);
  assert(references.length > 0, 'expected built asset references');
  for (const reference of references) {
    assert(existsSync(resolve(dist, reference)), `missing built asset ${reference}`);
  }
});

