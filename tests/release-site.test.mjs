import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  assertTreesByteEqual,
  createReleasePlan,
  parseArgs,
  parsePublishOutput,
  runRelease,
} from '../scripts/release-site.mjs';
import {
  configFor as workerAssetsConfigFor,
  parseArgs as parseWorkerAssetsArgs,
} from '../scripts/deploy-worker-assets.mjs';
import { resolveHtreeCommand } from '../scripts/hashtreePaths.mjs';
import staticAssetsWorker from '../scripts/https-static-assets-worker.mjs';

const originalHtreeBin = process.env.HTREE_BIN;

function restoreEnv(name, value) {
  if (value === undefined) {
    delete process.env[name];
  } else {
    process.env[name] = value;
  }
}

test.afterEach(() => {
  restoreEnv('HTREE_BIN', originalHtreeBin);
});

test('defaults to the stack.iris.to Worker custom domain and Hashtree site ref', () => {
  const parsed = parseArgs([], {});

  assert.equal(parsed.workerName, 'iris-stack');
  assert.equal(parsed.treeName, 'iris-stack-site');
  assert.deepEqual(parsed.routes, []);
  assert.deepEqual(parsed.domains, ['stack.iris.to']);
});

test('does not attach the production domain to an overridden Worker', () => {
  const parsed = parseArgs(['--worker-name', 'iris-stack-preview'], {});

  assert.equal(parsed.workerName, 'iris-stack-preview');
  assert.deepEqual(parsed.domains, []);
  assert.deepEqual(parsed.routes, []);
});

test('builds and tests one root dist before publishing and deploying it', () => {
  delete process.env.HTREE_BIN;

  const plan = createReleasePlan({
    workerName: 'iris-stack',
    treeName: 'iris-stack-site',
    routes: [],
    domains: ['stack.iris.to'],
    skipCloudflare: false,
    workerCompatibilityDate: '2026-07-15',
  });

  assert.deepEqual(
    plan.steps.map((step) => step.id),
    ['verify-htree', 'build', 'test-1', 'test-2', 'publish', 'deploy'],
  );
  assert.deepEqual(plan.steps[0].command, ['htree', '--version']);
  assert.deepEqual(plan.steps[1].command, ['pnpm', 'run', 'build']);
  assert.deepEqual(plan.steps[2].command, ['node', '--test', 'tests/portable-build.test.mjs']);
  assert.deepEqual(plan.steps[3].command, ['node', './scripts/portable-smoke.mjs']);
  assert.equal(plan.steps[0].cwd, plan.repoRoot);
  assert.equal(plan.steps[1].cwd, plan.repoRoot);
  assert.equal(plan.steps[2].cwd, plan.repoRoot);
  assert.equal(plan.steps[3].cwd, plan.repoRoot);
  assert.equal(plan.steps[4].cwd, plan.distDir);
  assert.deepEqual(plan.steps[4].command, ['htree', 'add', '.', '--publish', 'iris-stack-site']);
  assert.deepEqual(plan.steps[5].command, [
    'node',
    './scripts/deploy-worker-assets.mjs',
    '--script',
    'scripts/https-static-assets-worker.mjs',
    '--assets',
    'dist',
    '--name',
    'iris-stack',
    '--compatibility-date',
    '2026-07-15',
    '--wrangler-version',
    '4',
    '--domain',
    'stack.iris.to',
  ]);
  assert.equal(plan.steps[5].cwd, plan.repoRoot);
});

test('runs Hashtree publish and Cloudflare deploy in parallel only after dist tests', async () => {
  let activeReleaseSteps = 0;
  let maxActiveReleaseSteps = 0;
  const calls = [];
  let verificationStep;

  const result = await runRelease(
    {
      workerName: 'iris-stack',
      treeName: 'iris-stack-site',
      routes: [],
      domains: ['stack.iris.to'],
      skipCloudflare: false,
      workerCompatibilityDate: '2026-07-15',
    },
    async (step) => {
      calls.push(step.id);
      if (step.id === 'verify-publish') {
        verificationStep = step;
      }
      if (step.id === 'publish' || step.id === 'deploy') {
        activeReleaseSteps += 1;
        maxActiveReleaseSteps = Math.max(maxActiveReleaseSteps, activeReleaseSteps);
        await new Promise((resolve) => setTimeout(resolve, 10));
        activeReleaseSteps -= 1;
      }
      if (step.id === 'publish') {
        return {
          status: 0,
          stdout: 'published: npub1example/iris-stack-site\nnhash1ace',
          stderr: '',
        };
      }
      if (step.id === 'verify-htree') {
        return { status: 0, stdout: 'htree 0.2.88\n', stderr: '' };
      }
      return { status: 0, stdout: '', stderr: '' };
    },
    { buildOutputExists: () => true, verifyRetrievedTree: () => {} },
  );

  assert.deepEqual(calls, [
    'verify-htree',
    'build',
    'test-1',
    'test-2',
    'publish',
    'deploy',
    'verify-publish',
  ]);
  assert.equal(maxActiveReleaseSteps, 2);
  assert.deepEqual(result.publish, {
    nhash: 'nhash1ace',
    publishedRef: 'npub1example/iris-stack-site',
  });
  assert.equal(result.workerName, 'iris-stack');
  assert.equal(result.verified, true);
  assert.deepEqual(result.domains, ['stack.iris.to']);
  assert.equal(verificationStep.command[0], 'htree');
  assert(verificationStep.command.includes('nhash1ace'));
  assert(verificationStep.command.includes('get'));
  assert.match(verificationStep.env.HTREE_CONFIG_DIR, /iris-stack-site-verify-/);
});

test('stops before testing or releasing when the build did not create dist', async () => {
  const calls = [];

  await assert.rejects(
    runRelease(
      {
        workerName: 'iris-stack',
        treeName: 'iris-stack-site',
        routes: [],
        domains: ['stack.iris.to'],
        skipCloudflare: false,
        workerCompatibilityDate: '2026-07-15',
      },
      (step) => {
        calls.push(step.id);
        if (step.id === 'verify-htree') {
          return { status: 0, stdout: 'htree 0.2.88\n', stderr: '' };
        }
        return { status: 0, stdout: '', stderr: '' };
      },
      { buildOutputExists: () => false },
    ),
    /Build output directory not found/,
  );

  assert.deepEqual(calls, ['verify-htree', 'build']);
});

test('supports dry runs without executing commands', async () => {
  let ran = false;
  const options = parseArgs(['--dry-run'], {});
  const result = await runRelease(options, () => {
    ran = true;
    return { status: 0, stdout: '', stderr: '' };
  });

  assert.equal(result.dryRun, true);
  assert.equal(ran, false);
  assert.deepEqual(result.steps.map((step) => step.id), [
    'verify-htree',
    'build',
    'test-1',
    'test-2',
    'publish',
    'deploy',
  ]);
});

test('supports publishing to Hashtree without mutating Cloudflare', async () => {
  const options = parseArgs(['--skip-cloudflare'], {});
  const calls = [];
  const result = await runRelease(
    options,
    (step) => {
      calls.push(step.id);
      if (step.id === 'publish') {
        return {
          status: 0,
          stdout: 'published: npub1example/iris-stack-site\nnhash1ace',
          stderr: '',
        };
      }
      if (step.id === 'verify-htree') {
        return { status: 0, stdout: 'htree 0.2.88\n', stderr: '' };
      }
      return { status: 0, stdout: '', stderr: '' };
    },
    { buildOutputExists: () => true, verifyRetrievedTree: () => {} },
  );

  assert.deepEqual(calls, [
    'verify-htree',
    'build',
    'test-1',
    'test-2',
    'publish',
    'verify-publish',
  ]);
  assert.equal(result.workerName, null);
  assert.deepEqual(result.domains, []);
});

test('parses Hashtree publish output defensively', () => {
  assert.deepEqual(parsePublishOutput('published: npub1foo/iris-stack-site\nnhash1ace'), {
    nhash: 'nhash1ace',
    publishedRef: 'npub1foo/iris-stack-site',
  });
  assert.throws(
    () => parsePublishOutput('published: npub1foo/iris-stack-site'),
    /Publish succeeded but no nhash was found in htree output/,
  );
  assert.throws(
    () => parsePublishOutput('nhash1ace'),
    /Publish succeeded but no mutable ref was found in htree output/,
  );
});

test('rejects any release CLI other than exact hashtree-cli 0.2.88', async () => {
  const options = parseArgs(['--skip-cloudflare'], {});
  await assert.rejects(
    runRelease(options, () => ({ status: 0, stdout: 'htree 0.2.87\n', stderr: '' })),
    /requires htree 0\.2\.88/,
  );
});

test('fails closed when the fresh Hashtree retrieval fails', async () => {
  const calls = [];
  const options = parseArgs([], {});
  await assert.rejects(
    runRelease(
      options,
      (step) => {
        calls.push(step.id);
        if (step.id === 'verify-htree') {
          return { status: 0, stdout: 'htree 0.2.88\n', stderr: '' };
        }
        if (step.id === 'publish') {
          return {
            status: 0,
            stdout: 'published: npub1example/iris-stack-site\nnhash1ace',
            stderr: '',
          };
        }
        return { status: step.id === 'verify-publish' ? 1 : 0, stdout: '', stderr: '' };
      },
      { buildOutputExists: () => true },
    ),
    /Verify published Hashtree tree .* failed with exit code 1/,
  );
  assert(
    calls.includes('deploy'),
    'Cloudflare and Hashtree release steps should stay independent',
  );
  assert.equal(calls.at(-1), 'verify-publish');
});

test('fresh-publish verifier compares both paths and file bytes', () => {
  const root = mkdtempSync(path.join(tmpdir(), 'iris-stack-tree-compare-'));
  const expected = path.join(root, 'expected');
  const actual = path.join(root, 'actual');
  try {
    mkdirSync(path.join(expected, 'assets'), { recursive: true });
    mkdirSync(path.join(actual, 'assets'), { recursive: true });
    writeFileSync(path.join(expected, 'index.html'), 'same');
    writeFileSync(path.join(actual, 'index.html'), 'same');
    writeFileSync(path.join(expected, 'assets', 'app.js'), 'one');
    writeFileSync(path.join(actual, 'assets', 'app.js'), 'one');
    assert.doesNotThrow(() => assertTreesByteEqual(expected, actual));
    writeFileSync(path.join(actual, 'extra.txt'), 'extra');
    assert.throws(() => assertTreesByteEqual(expected, actual), /does not contain/);
    rmSync(path.join(actual, 'extra.txt'));
    writeFileSync(path.join(actual, 'assets', 'app.js'), 'two');
    assert.throws(() => assertTreesByteEqual(expected, actual), /differs from dist/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('generates Worker Static Assets config for the root dist', () => {
  const options = parseWorkerAssetsArgs([
    '--script',
    'scripts/https-static-assets-worker.mjs',
    '--assets',
    'dist',
    '--name',
    'iris-stack',
    '--compatibility-date',
    '2026-07-15',
    '--domain',
    'stack.iris.to',
  ]);

  assert.deepEqual(workerAssetsConfigFor(options), {
    name: 'iris-stack',
    compatibility_date: '2026-07-15',
    main: 'scripts/https-static-assets-worker.mjs',
    assets: {
      directory: 'dist',
      binding: 'ASSETS',
      run_worker_first: true,
    },
  });
  assert.deepEqual(options.domains, ['stack.iris.to']);
  assert.deepEqual(options.routes, []);
});

test('redirect Worker upgrades HTTP and delegates HTTPS to the exact static asset binding', async () => {
  let delegated = false;
  const env = {
    ASSETS: {
      fetch(request) {
        delegated = true;
        return new Response(new URL(request.url).pathname, { status: 200 });
      },
    },
  };

  const redirect = await staticAssetsWorker.fetch(
    new Request('http://stack.iris.to/docs?mode=full'),
    env,
  );
  assert.equal(redirect.status, 308);
  assert.equal(
    redirect.headers.get('location'),
    'https://stack.iris.to/docs?mode=full',
  );
  assert.equal(delegated, false);

  const asset = await staticAssetsWorker.fetch(
    new Request('https://stack.iris.to/assets/site.js'),
    env,
  );
  assert.equal(asset.status, 200);
  assert.equal(await asset.text(), '/assets/site.js');
  assert.equal(delegated, true);
});

test('uses an installed htree by default and honors an explicit binary override', () => {
  delete process.env.HTREE_BIN;
  assert.deepEqual(resolveHtreeCommand('add', '.'), ['htree', 'add', '.']);

  process.env.HTREE_BIN = '/tmp/htree-test-bin';
  assert.deepEqual(resolveHtreeCommand('add', '.'), ['/tmp/htree-test-bin', 'add', '.']);
});
