export function resolveHtreeCommand(...args) {
  return [process.env.HTREE_BIN || 'htree', ...args];
}
