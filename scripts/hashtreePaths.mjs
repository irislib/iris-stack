import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const repoRoot = path.resolve(__dirname, '..');

function isHashtreeRepo(candidate) {
  return Boolean(candidate && existsSync(path.join(candidate, 'rust', 'Cargo.toml')));
}

function isRustWorkspace(candidate) {
  return Boolean(candidate && existsSync(path.join(candidate, 'Cargo.toml')));
}

export function resolveHashtreeRepoRoot() {
  const candidate = process.env.HASHTREE_REPO_ROOT;
  return isHashtreeRepo(candidate) ? candidate : null;
}

export function resolveHashtreeRustDir() {
  const explicitRustDir = process.env.HASHTREE_RUST_DIR;
  if (isRustWorkspace(explicitRustDir)) {
    return explicitRustDir;
  }

  const hashtreeRepoRoot = resolveHashtreeRepoRoot();
  if (!hashtreeRepoRoot) {
    return null;
  }

  const rustDir = path.join(hashtreeRepoRoot, 'rust');
  return isRustWorkspace(rustDir) ? rustDir : null;
}

export function resolveHtreeCommand(...args) {
  if (process.env.HTREE_BIN) {
    return [process.env.HTREE_BIN, ...args];
  }

  const rustDir = resolveHashtreeRustDir();
  if (rustDir) {
    return [
      'cargo',
      'run',
      '--manifest-path',
      path.join(rustDir, 'Cargo.toml'),
      '-p',
      'hashtree-cli',
      '--bin',
      'htree',
      '--',
      ...args,
    ];
  }

  return ['htree', ...args];
}
