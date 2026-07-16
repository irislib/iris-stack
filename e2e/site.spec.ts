import { expect, test } from '@playwright/test';

test('renders the public guide', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });

  await page.goto('/');

  await expect(page).toHaveTitle(/Iris Stack/);
  await expect(page.getByRole('heading', { level: 1, name: 'Iris Stack' })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Table of contents' })).toBeVisible();
  await expect(page.locator('#table-of-contents a')).not.toHaveCount(0);
  expect(errors).toEqual([]);
});

test('keeps the document usable on a narrow phone viewport', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 700 });
  await page.goto('/');

  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  const tableOfContents = page.getByRole('navigation', { name: 'Table of contents' });
  await expect(tableOfContents).toBeVisible();
  const contents = page.getByRole('button', { name: 'Contents' });
  await expect(contents).toBeVisible();
  await expect(contents).toHaveAttribute('aria-expanded', 'false');
  const firstTocLink = tableOfContents.locator('a').first();
  await expect(firstTocLink).toBeHidden();
  await contents.click();
  await expect(contents).toHaveAttribute('aria-expanded', 'true');
  await expect(firstTocLink).toBeVisible();
  const linkPositions = await tableOfContents.locator('a').evaluateAll((links) => links.map((link) => link.getBoundingClientRect().top));
  expect(linkPositions.every((top, index) => index === 0 || top > linkPositions[index - 1])).toBe(true);

  const tableMetrics = await page.locator('.markdown table').evaluateAll((tables) =>
    tables.map((table) => ({
      clientWidth: table.clientWidth,
      scrollWidth: table.scrollWidth,
      unlabeledCells: [...table.querySelectorAll('tbody td')].filter((cell) => !cell.getAttribute('data-label')).length,
    })),
  );
  expect(tableMetrics.every(({ clientWidth, scrollWidth }) => scrollWidth <= clientWidth + 1)).toBe(true);
  expect(tableMetrics.every(({ unlabeledCells }) => unlabeledCells === 0)).toBe(true);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(1);
});

test('keeps deep links aligned after document rendering', async ({ page }) => {
  await page.goto('/');
  const target = await page.locator('#table-of-contents a').first().getAttribute('href');
  expect(target).toMatch(/^#[a-z0-9-]+$/);
  await page.goto(`/${target}`);

  const heading = page.locator(target!).locator('..');
  await expect.poll(async () => {
    const top = await heading.evaluate((element) => element.getBoundingClientRect().top);
    return top >= 0 && top <= 120;
  }).toBe(true);
});

test('marks the section occupying the viewport center in the table of contents', async ({ page }) => {
  await page.goto('/');

  const activeTarget = page.locator('#table-of-contents a').nth(2);
  const target = await activeTarget.getAttribute('href');
  expect(target).toMatch(/^#[a-z0-9-]+$/);
  const inactiveMetrics = await activeTarget.evaluate((element) => ({
    fontWeight: getComputedStyle(element).fontWeight,
    height: element.getBoundingClientRect().height,
  }));

  await page.locator(target!).evaluate((element) => {
    element.parentElement?.scrollIntoView({ block: 'center' });
  });

  await expect(activeTarget).toHaveAttribute('aria-current', 'location');
  const activeMetrics = await activeTarget.evaluate((element) => ({
    fontWeight: getComputedStyle(element).fontWeight,
    height: element.getBoundingClientRect().height,
  }));
  expect(activeMetrics).toEqual(inactiveMetrics);
});
