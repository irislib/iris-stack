import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import UnoCSS from 'unocss/vite';
import { defineConfig, type Plugin } from 'vite';
import unoConfig from './uno.config';

const siteDir = dirname(fileURLToPath(import.meta.url));

export function sanitizePortableHtml(html: string): string {
  return html
    .replace(/^\s*<link rel="modulepreload".*$/gm, '')
    .replace(/\s+crossorigin(?=[\s>])/g, '');
}

function portableHtmlPlugin(): Plugin {
  return {
    name: 'portable-html',
    async closeBundle() {
      const indexPath = resolve(siteDir, '..', 'dist', 'index.html');
      try {
        const html = await readFile(indexPath, 'utf8');
        await writeFile(indexPath, sanitizePortableHtml(html), 'utf8');
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
      }
    },
  };
}

export default defineConfig({
  root: siteDir,
  base: './',
  plugins: [portableHtmlPlugin(), UnoCSS(unoConfig), svelte()],
  build: {
    outDir: resolve(siteDir, '..', 'dist'),
    emptyOutDir: true,
    modulePreload: false,
    reportCompressedSize: true,
  },
  server: {
    port: 5179,
  },
});
