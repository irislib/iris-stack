<script lang="ts">
  import { onMount } from 'svelte';
  import { marked } from 'marked';
  import guideSource from '../../docs/iris-stack.md?raw';

  const owner = 'npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm';
  const sourceUrl = `https://git.iris.to/#/${owner}/iris-stack`;
  let tocOpen = false;
  let activeTocId = '';

  function plainHeading(markdown: string): string {
    return markdown
      .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
      .replace(/[`*_~]/g, '')
      .trim();
  }

  function slugify(value: string): string {
    return plainHeading(value)
      .replace(/^\d+(?:\.\d+)*[.)]?\s+/, '')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/(^-|-$)/g, '');
  }

  const headings = [...guideSource.matchAll(/^(#{2,3}) (.+)$/gm)]
    .map((match) => ({
      level: match[1].length,
      label: plainHeading(match[2]),
      id: slugify(match[2]),
    }))
    .filter((heading) => heading.label !== 'A Freedom Tech Toolkit');

  const chapters = headings
    .filter((heading) => heading.level === 2)
    .map((heading, index, topLevel) => {
      const start = headings.indexOf(heading) + 1;
      const next = topLevel[index + 1];
      const end = next ? headings.indexOf(next) : headings.length;
      return { ...heading, children: headings.slice(start, end) };
    });

  const markdown = guideSource
    .replace(/^(#{2,3}) (.+)$/gm, (_, marks: string, heading: string) => `${marks} <span class="heading-anchor" id="${slugify(heading)}"></span>${heading}`)
    .replace(/\]\(\.\.\/stack\.json\)/g, `](${sourceUrl}/stack.json)`)
    .replace(/\]\(integration-lab\.md\)/g, `](${sourceUrl}/docs/integration-lab.md)`);

  const documentHtml = (marked.parse(markdown, { gfm: true }) as string)
    .replace('<h1>', '<h1><img class="title-icon" src="./favicon.svg" alt="" />');

  function labelTableCells(): void {
    document.querySelectorAll<HTMLTableElement>('.markdown table').forEach((table) => {
      const labels = [...table.querySelectorAll<HTMLTableCellElement>('thead th')].map((header) => header.textContent?.trim() ?? '');
      table.querySelectorAll<HTMLTableRowElement>('tbody tr').forEach((row) => {
        [...row.cells].forEach((cell, index) => {
          const label = labels[index] ?? '';
          const labelElement = document.createElement('span');
          const valueElement = document.createElement('span');

          cell.dataset.label = label;
          labelElement.className = 'table-cell-label';
          labelElement.setAttribute('aria-hidden', 'true');
          labelElement.textContent = label;
          valueElement.className = 'table-cell-value';
          valueElement.append(...cell.childNodes);
          cell.append(labelElement, valueElement);
        });
      });
    });
  }

  function updateActiveToc(): void {
    const viewportCenter = window.innerHeight / 2;
    let nextActiveId = '';

    document.querySelectorAll<HTMLElement>('.heading-anchor').forEach((anchor) => {
      const headingTop = anchor.parentElement?.getBoundingClientRect().top;
      if (headingTop !== undefined && headingTop <= viewportCenter) {
        nextActiveId = anchor.id;
      }
    });

    activeTocId = nextActiveId;
  }

  onMount(() => {
    labelTableCells();
    let scrollFrame = 0;
    const onScroll = () => {
      if (scrollFrame) return;
      scrollFrame = window.requestAnimationFrame(() => {
        scrollFrame = 0;
        updateActiveToc();
      });
    };

    window.addEventListener('scroll', onScroll, { passive: true });
    updateActiveToc();

    const targetId = decodeURIComponent(window.location.hash.slice(1));
    if (targetId) {
      void document.fonts.ready.then(() => {
        window.requestAnimationFrame(() => {
          document.getElementById(targetId)?.parentElement?.scrollIntoView({
            behavior: 'instant',
            block: 'start',
          });
          updateActiveToc();
        });
      });
    }

    return () => {
      window.removeEventListener('scroll', onScroll);
      if (scrollFrame) window.cancelAnimationFrame(scrollFrame);
    };
  });
</script>

<svelte:head>
  <meta name="robots" content="index,follow" />
</svelte:head>

<a class="skip-link" href="#document">Skip to document</a>

<main id="top" class="doc-layout mx-auto max-w-6xl px-5">
  <aside>
    <nav aria-label="Table of contents">
      <button class="toc-toggle" type="button" aria-expanded={tocOpen} aria-controls="table-of-contents" onclick={() => (tocOpen = !tocOpen)}>Contents</button>
      <ol id="table-of-contents" class:toc-open={tocOpen}>
        {#each chapters as chapter}
          <li>
            <a href={`#${chapter.id}`} class:active={activeTocId === chapter.id} aria-current={activeTocId === chapter.id ? 'location' : undefined}>{chapter.label}</a>
            {#if chapter.children.length}
              <ol>
                {#each chapter.children as heading}
                  <li><a href={`#${heading.id}`} class:active={activeTocId === heading.id} aria-current={activeTocId === heading.id ? 'location' : undefined}>{heading.label}</a></li>
                {/each}
              </ol>
            {/if}
          </li>
        {/each}
      </ol>
    </nav>
  </aside>

  <article id="document" class="markdown" tabindex="-1">
    {@html documentHtml}
  </article>
</main>

<footer class="mx-auto max-w-6xl px-5">
  <a href={sourceUrl} target="_blank" rel="noreferrer">Source ↗</a>
</footer>
