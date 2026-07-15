import { expect, test } from '@playwright/test';

test('renders the public architecture Markdown', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });

  await page.goto('/');

  await expect(page).toHaveTitle(/Iris Stack/);
  await expect(page.getByRole('heading', { level: 1, name: 'Iris Stack' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Table of contents' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '1. Capability layers' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '3.4 Social graph as local policy' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '3.5 Human names without a global namespace' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '4.2 Hashtree indexes for large datasets' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '4.3 Web apps and updates as verified trees' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '6. Products' })).toBeVisible();
  await expect(page.locator('.mermaid')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Product page' }).first()).toHaveAttribute('href', 'https://irischat.org/');
  await expect(page.locator('.hero')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('keeps the document usable on a narrow phone viewport', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 700 });
  await page.goto('/');

  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Document links' })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(1);
});

test('keeps deep links aligned after document rendering', async ({ page }) => {
  await page.goto('/#hashtree-indexes-for-large-datasets');

  const heading = page.getByRole('heading', { name: '4.2 Hashtree indexes for large datasets' });
  const top = await heading.evaluate((element) => element.getBoundingClientRect().top);
  expect(top).toBeGreaterThanOrEqual(0);
  expect(top).toBeLessThanOrEqual(120);
});
