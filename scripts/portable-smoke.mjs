import http from 'node:http';
import { mkdir, readFile } from 'node:fs/promises';
import path from 'node:path';

import { chromium } from '@playwright/test';

const repoRoot = path.resolve(import.meta.dirname, '..');
const distDir = path.join(repoRoot, 'dist');
const testResultsDir = path.join(repoRoot, 'test-results');
const screenshotPath = path.join(testResultsDir, 'iris-stack-portable-smoke.png');
const prefix = '/portable';

const MIME_TYPES = new Map([
  ['.css', 'text/css; charset=utf-8'],
  ['.html', 'text/html; charset=utf-8'],
  ['.ico', 'image/x-icon'],
  ['.js', 'application/javascript; charset=utf-8'],
  ['.json', 'application/json; charset=utf-8'],
  ['.mjs', 'application/javascript; charset=utf-8'],
  ['.png', 'image/png'],
  ['.svg', 'image/svg+xml'],
  ['.wasm', 'application/wasm'],
  ['.webmanifest', 'application/manifest+json; charset=utf-8'],
  ['.webp', 'image/webp'],
  ['.woff2', 'font/woff2'],
]);

function contentTypeFor(filePath) {
  return MIME_TYPES.get(path.extname(filePath)) ?? 'application/octet-stream';
}

function safeJoin(rootDir, requestPath) {
  if (!requestPath.startsWith(prefix)) {
    throw new Error(`Unsupported portable path: ${requestPath}`);
  }
  const relativePath = requestPath.slice(prefix.length) || '/index.html';
  const normalized = relativePath === '/' ? '/index.html' : relativePath;
  const fullPath = path.resolve(rootDir, `.${normalized}`);
  if (!fullPath.startsWith(`${rootDir}${path.sep}`)) {
    throw new Error(`Refusing to serve path outside dist: ${requestPath}`);
  }
  return fullPath;
}

async function startServer(rootDir) {
  const server = http.createServer(async (request, response) => {
    try {
      const requestUrl = new URL(request.url ?? `${prefix}/`, 'http://127.0.0.1');
      const filePath = safeJoin(rootDir, decodeURIComponent(requestUrl.pathname));
      const body = await readFile(filePath);
      response.writeHead(200, {
        'content-type': contentTypeFor(filePath),
        'cache-control': 'no-store',
      });
      response.end(body);
    } catch (error) {
      response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
      response.end(error instanceof Error ? error.message : 'not found');
    }
  });

  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const address = server.address();
  if (!address || typeof address === 'string') {
    server.close();
    throw new Error('Failed to determine portable smoke server address');
  }
  return {
    server,
    url: `http://127.0.0.1:${address.port}${prefix}/index.html`,
  };
}

const { server, url } = await startServer(distDir);
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const pageErrors = [];
const consoleErrors = [];

page.on('pageerror', (error) => {
  pageErrors.push(error.stack || error.message);
});
page.on('console', (message) => {
  if (message.type() === 'error') {
    consoleErrors.push(message.text());
  }
});

try {
  const response = await page.goto(url, { waitUntil: 'load', timeout: 60_000 });
  if (!response || response.status() !== 200) {
    throw new Error(`Portable build returned ${response?.status() ?? 'no response'} for ${url}`);
  }

  await page.locator('h1').waitFor({ state: 'visible', timeout: 15_000 });
  const title = await page.title();
  if (!/Iris Stack/i.test(title)) {
    throw new Error(`Portable build loaded unexpected title "${title}"`);
  }
  const heading = (await page.locator('h1').first().textContent())?.trim();
  if (!heading) {
    throw new Error('Portable build rendered an empty primary heading');
  }

  await mkdir(testResultsDir, { recursive: true });
  await page.screenshot({ path: screenshotPath });

  if (pageErrors.length > 0) {
    throw new Error(`Portable build hit page errors:\n${pageErrors.join('\n')}`);
  }
  if (consoleErrors.length > 0) {
    throw new Error(`Portable build logged console errors:\n${consoleErrors.join('\n')}`);
  }

  console.log(`Portable stack.iris.to smoke passed: ${url}`);
  console.log(`Screenshot: ${screenshotPath}`);
} finally {
  await browser.close();
  await new Promise((resolve) => server.close(resolve));
}
