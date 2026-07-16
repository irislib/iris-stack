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
  await expect(page.locator('.title-icon')).toBeVisible();
  await expect(page.getByRole('banner')).toHaveCount(0);
  await expect(page.getByRole('navigation', { name: 'Table of contents' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '1. Capability layers' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '2. Identity' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '2.1 Nostr identity and signed events' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '2.2 Signed fact events' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '4.1 nostr-pubsub publish-subscribe' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '4.2 Signed peer and service discovery' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '5.2 Hashtree indexes for large datasets' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '5.3 Web apps and updates as verified trees' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '6.1 Social graph as local policy' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '6.2 Human names without a global namespace' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '7. Payments' })).toBeVisible();
  await expect(page.getByRole('heading', { name: '8. Products' })).toBeVisible();
  await expect(page.locator('.mermaid')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Product page' }).first()).toHaveAttribute('href', 'https://irischat.org/');
  await expect(page.locator('.hero')).toHaveCount(0);

  const title = page.getByRole('heading', { level: 1, name: 'Iris Stack' });
  const subtitle = page.getByRole('heading', { level: 2, name: 'A Freedom Tech Toolkit' });
  const titleBottom = await title.evaluate((element) => element.getBoundingClientRect().bottom);
  const subtitleTop = await subtitle.evaluate((element) => element.getBoundingClientRect().top);
  expect(subtitleTop - titleBottom).toBeLessThanOrEqual(40);
  const tableWidths = await page.locator('.markdown table').evaluateAll((tables) =>
    tables.map((table) => ({
      table: table.getBoundingClientRect().width,
      row: table.querySelector('tr')?.getBoundingClientRect().width ?? 0,
    })),
  );
  expect(tableWidths.every(({ table, row }) => Math.abs(table - row) <= 1)).toBe(true);
  expect(errors).toEqual([]);
});

test('keeps the document usable on a narrow phone viewport', async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 700 });
  await page.goto('/');

  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await expect(page.getByRole('navigation', { name: 'Document links' })).toHaveCount(0);
  const tableOfContents = page.getByRole('navigation', { name: 'Table of contents' });
  await expect(tableOfContents).toBeVisible();
  const contents = page.getByRole('button', { name: 'Contents' });
  await expect(contents).toBeVisible();
  await expect(contents).toHaveAttribute('aria-expanded', 'false');
  const subsection = page.getByRole('link', { name: '5.2 Hashtree indexes for large datasets' });
  await expect(subsection).toBeHidden();
  await contents.click();
  await expect(contents).toHaveAttribute('aria-expanded', 'true');
  await expect(subsection).toBeVisible();
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
  await page.goto('/#hashtree-indexes-for-large-datasets');

  const heading = page.getByRole('heading', { name: '5.2 Hashtree indexes for large datasets' });
  await expect.poll(async () => {
    const top = await heading.evaluate((element) => element.getBoundingClientRect().top);
    return top >= 0 && top <= 120;
  }).toBe(true);
});

test('keeps a clicked section active when anchor navigation aligns it to the top', async ({ page }) => {
  await page.goto('/');

  const firstTarget = page.locator('#table-of-contents a[href="#nostr-identity-and-signed-events"]');
  const inactiveMetrics = await firstTarget.evaluate((element) => ({
    fontWeight: getComputedStyle(element).fontWeight,
    height: element.getBoundingClientRect().height,
  }));

  for (const id of [
    'nostr-identity-and-signed-events',
    'signed-fact-events',
    'nostr-pubsub-publish-subscribe',
    'signed-peer-and-service-discovery',
  ]) {
    const target = page.locator(`#table-of-contents a[href="#${id}"]`);
    await target.click();
    await page.evaluate(() => new Promise<void>((resolve) => {
      let previousY = window.scrollY;
      let stableFrames = 0;
      const check = () => {
        const nextY = window.scrollY;
        stableFrames = nextY === previousY ? stableFrames + 1 : 0;
        previousY = nextY;
        if (stableFrames >= 3) resolve();
        else window.requestAnimationFrame(check);
      };
      window.requestAnimationFrame(check);
    }));

    await expect(target).toHaveAttribute('aria-current', 'location');
    await expect(page.locator('#table-of-contents [aria-current="location"]')).toHaveCount(1);
  }

  const activeMetrics = await firstTarget.evaluate((element) => ({
    fontWeight: getComputedStyle(element).fontWeight,
    height: element.getBoundingClientRect().height,
  }));
  expect(activeMetrics).toEqual(inactiveMetrics);
});
