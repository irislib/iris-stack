<script lang="ts">
  import { onMount } from 'svelte';
  import { marked } from 'marked';
  import guideSource from '../../docs/iris-stack.md?raw';

  const owner = 'npub1xdhnr9mrv47kkrn95k6cwecearydeh8e895990n3acntwvmgk2dsdeeycm';
  const sourceUrl = `https://git.iris.to/#/${owner}/iris-stack`;

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

  const headings = [...guideSource.matchAll(/^(#{2,3}) (.+)$/gm)].map((match) => ({
    level: match[1].length,
    label: plainHeading(match[2]),
    id: slugify(match[2]),
  }));

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

  const documentHtml = marked.parse(markdown, { gfm: true }) as string;

  onMount(() => {
    const targetId = decodeURIComponent(window.location.hash.slice(1));
    if (targetId) {
      window.requestAnimationFrame(() => {
        document.getElementById(targetId)?.parentElement?.scrollIntoView({
          behavior: 'instant',
          block: 'start',
        });
      });
    }
  });
</script>

<svelte:head>
  <meta name="robots" content="index,follow" />
</svelte:head>

<a class="skip-link" href="#document">Skip to document</a>

<header class="topbar">
  <div class="topbar-inner mx-auto max-w-6xl px-5">
    <a href="#top" class="site-name"><img src="./favicon.svg" alt="" />Iris Stack</a>
    <nav aria-label="Document links">
      <a href={sourceUrl} target="_blank" rel="noreferrer">Source ↗</a>
    </nav>
  </div>
</header>

<main class="doc-layout mx-auto max-w-6xl px-5">
  <aside>
    <nav aria-label="Table of contents">
      <span>Contents</span>
      <ol>
        {#each chapters as chapter}
          <li>
            <a href={`#${chapter.id}`}>{chapter.label}</a>
            {#if chapter.children.length}
              <ol>
                {#each chapter.children as heading}
                  <li><a href={`#${heading.id}`}>{heading.label}</a></li>
                {/each}
              </ol>
            {/if}
          </li>
        {/each}
      </ol>
    </nav>
  </aside>

  <article id="document" class="markdown" tabindex="-1">
    <span id="top"></span>
    {@html documentHtml}
  </article>
</main>

<footer>
  <div class="mx-auto max-w-6xl px-5">
    <span>Iris Stack · 0BSD</span>
  </div>
</footer>
