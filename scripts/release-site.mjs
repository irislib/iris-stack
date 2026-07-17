import { spawn } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { resolveHtreeCommand } from './hashtreePaths.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..');
const defaultWorkerCompatibilityDate = '2026-07-15';
const wranglerVersion = '4.111.0';
const requiredHtreeVersion = '0.2.97';

export const releaseProfile = {
  appName: 'stack.iris.to',
  distDir: 'dist',
  treeName: 'iris-stack-site',
  defaultWorkerName: 'iris-stack',
  defaultRoutes: [],
  defaultDomains: ['stack.iris.to'],
  workerScript: 'scripts/https-static-assets-worker.mjs',
  workerNameEnv: 'CF_WORKER_NAME_IRIS_STACK',
  buildCommand: ['pnpm', 'run', 'build'],
  testCommands: [
    ['node', '--test', 'tests/portable-build.test.mjs'],
    ['node', './scripts/portable-smoke.mjs'],
  ],
};

function clone(values) {
  return values ? [...values] : [];
}

function takeValue(args, option) {
  const value = args.shift();
  if (!value || value.startsWith('--')) {
    throw new Error(`Missing value for ${option}`);
  }
  return value;
}

function usesProductionWorker(workerName) {
  return workerName === releaseProfile.defaultWorkerName;
}

export function parseArgs(argv, env = process.env) {
  const args = [...argv].filter((arg, index) => !(arg === '--' && index === 0));
  let workerName;
  let treeName;
  let workerCompatibilityDate;
  let dryRun = false;
  let skipCloudflare = false;
  const routes = [];
  const domains = [];

  while (args.length > 0) {
    const arg = args.shift();
    if (arg === '-h' || arg === '--help') {
      return { help: true };
    }
    if (arg === '--') {
      continue;
    }
    if (arg === '--worker-name') {
      workerName = takeValue(args, arg);
      continue;
    }
    if (arg === '--tree') {
      treeName = takeValue(args, arg);
      continue;
    }
    if (arg === '--route') {
      routes.push(takeValue(args, arg));
      continue;
    }
    if (arg === '--domain') {
      domains.push(takeValue(args, arg));
      continue;
    }
    if (arg === '--compatibility-date') {
      workerCompatibilityDate = takeValue(args, arg);
      continue;
    }
    if (arg === '--skip-cloudflare') {
      skipCloudflare = true;
      continue;
    }
    if (arg === '--dry-run') {
      dryRun = true;
      continue;
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  const resolvedWorkerName =
    workerName ?? env[releaseProfile.workerNameEnv] ?? releaseProfile.defaultWorkerName;

  return {
    dryRun,
    skipCloudflare,
    treeName: treeName ?? releaseProfile.treeName,
    workerName: resolvedWorkerName,
    routes:
      routes.length > 0
        ? routes
        : usesProductionWorker(resolvedWorkerName)
          ? clone(releaseProfile.defaultRoutes)
          : [],
    domains:
      domains.length > 0
        ? domains
        : usesProductionWorker(resolvedWorkerName)
          ? clone(releaseProfile.defaultDomains)
          : [],
    workerCompatibilityDate:
      workerCompatibilityDate ??
      env.CF_WORKER_COMPATIBILITY_DATE ??
      defaultWorkerCompatibilityDate,
  };
}

function workerDeployCommand(options) {
  const command = [
    'node',
    './scripts/deploy-worker-assets.mjs',
    '--script',
    releaseProfile.workerScript,
    '--assets',
    releaseProfile.distDir,
    '--name',
    options.workerName,
    '--compatibility-date',
    options.workerCompatibilityDate,
    '--wrangler-version',
    wranglerVersion,
  ];
  for (const route of options.routes ?? []) {
    command.push('--route', route);
  }
  for (const domain of options.domains ?? []) {
    command.push('--domain', domain);
  }
  return command;
}

export function createReleasePlan(options) {
  if (!options.skipCloudflare && !options.workerName) {
    throw new Error(
      `Missing Cloudflare Worker target. Pass --worker-name or set ${releaseProfile.workerNameEnv}.`,
    );
  }

  const distDir = path.join(repoRoot, releaseProfile.distDir);
  const steps = [
    {
      id: 'verify-htree',
      label: `Verify htree ${requiredHtreeVersion}`,
      command: resolveHtreeCommand('--version'),
      cwd: repoRoot,
    },
    {
      id: 'build',
      label: `Build ${releaseProfile.appName}`,
      command: releaseProfile.buildCommand,
      cwd: repoRoot,
    },
    ...releaseProfile.testCommands.map((command, index) => ({
      id: `test-${index + 1}`,
      label: `Test built ${releaseProfile.appName} (${index + 1}/${releaseProfile.testCommands.length})`,
      command,
      cwd: repoRoot,
    })),
    {
      id: 'publish',
      label: `Publish ${releaseProfile.appName} to Hashtree`,
      command: resolveHtreeCommand('add', '.', '--publish', options.treeName),
      cwd: distDir,
    },
  ];

  if (!options.skipCloudflare) {
    steps.push({
      id: 'deploy',
      label: `Deploy ${releaseProfile.appName} to Cloudflare Worker`,
      command: workerDeployCommand(options),
      cwd: repoRoot,
    });
  }

  return { profile: releaseProfile, repoRoot, distDir, steps };
}

function defaultRunner(step) {
  const [command, ...args] = step.command;
  console.log(`\n==> ${step.label}`);
  console.log(`$ ${[command, ...args].join(' ')}`);
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: step.cwd,
      env: step.env ? { ...process.env, ...step.env } : undefined,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';

    child.stdout?.setEncoding('utf8');
    child.stdout?.on('data', (chunk) => {
      stdout += chunk;
      process.stdout.write(chunk);
    });
    child.stderr?.setEncoding('utf8');
    child.stderr?.on('data', (chunk) => {
      stderr += chunk;
      process.stderr.write(chunk);
    });
    child.on('error', reject);
    child.on('close', (code, signal) => {
      if (signal) {
        const message = `Process exited with signal ${signal}\n`;
        stderr += message;
        process.stderr.write(message);
      }
      resolve({ status: code ?? 1, stdout, stderr });
    });
  });
}

function assertStepSucceeded(step, result) {
  if (result.status !== 0) {
    throw new Error(`${step.label} failed with exit code ${result.status}`);
  }
}

function assertHtreeVersion(result) {
  const output = `${result.stdout}\n${result.stderr}`;
  const escaped = requiredHtreeVersion.replaceAll('.', '\\.');
  if (!new RegExp(`^(?:htree|hashtree-cli) ${escaped}$`, 'm').test(output)) {
    throw new Error(
      `Site publication requires htree ${requiredHtreeVersion}; received ${output.trim() || 'no version output'}`,
    );
  }
}

function treeEntries(root, relative = '') {
  const directory = path.join(root, relative);
  return readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name))
    .flatMap((entry) => {
      const entryRelative = path.join(relative, entry.name);
      if (entry.isDirectory()) {
        return [`directory:${entryRelative}`, ...treeEntries(root, entryRelative)];
      }
      if (entry.isFile()) {
        return [`file:${entryRelative}`];
      }
      throw new Error(`Published tree contains unsupported entry: ${entryRelative}`);
    });
}

export function assertTreesByteEqual(expectedRoot, actualRoot) {
  const expectedEntries = treeEntries(expectedRoot);
  const actualEntries = treeEntries(actualRoot);
  if (expectedEntries.join('\n') !== actualEntries.join('\n')) {
    throw new Error('Fresh Hashtree retrieval does not contain the published dist file tree');
  }
  for (const entry of expectedEntries) {
    if (!entry.startsWith('file:')) {
      continue;
    }
    const relative = entry.slice('file:'.length);
    if (
      !readFileSync(path.join(expectedRoot, relative)).equals(
        readFileSync(path.join(actualRoot, relative)),
      )
    ) {
      throw new Error(`Fresh Hashtree retrieval differs from dist: ${relative}`);
    }
  }
}

function isReleaseStep(step) {
  return step.id === 'publish' || step.id === 'deploy';
}

export function parsePublishOutput(output) {
  const nhashMatch = output.match(/nhash1[ac-hj-np-z02-9]+/i);
  if (!nhashMatch) {
    throw new Error('Publish succeeded but no nhash was found in htree output');
  }
  const publishedMatch = output.match(/^\s*published:\s+(\S+)\s*$/im);
  if (!publishedMatch) {
    throw new Error('Publish succeeded but no mutable ref was found in htree output');
  }
  return { nhash: nhashMatch[0], publishedRef: publishedMatch[1] };
}

export async function runRelease(options, runner = defaultRunner, hooks = {}) {
  const plan = createReleasePlan(options);
  const buildOutputExists = hooks.buildOutputExists ?? existsSync;

  if (options.dryRun) {
    return { dryRun: true, profile: plan.profile, steps: plan.steps };
  }

  const prereleaseSteps = plan.steps.filter((step) => !isReleaseStep(step));
  const releaseSteps = plan.steps.filter(isReleaseStep);

  for (const step of prereleaseSteps) {
    const result = await runner(step);
    assertStepSucceeded(step, result);
    if (step.id === 'verify-htree') {
      assertHtreeVersion(result);
    }
    if (step.id === 'build' && !buildOutputExists(plan.distDir)) {
      throw new Error(`Build output directory not found: ${plan.distDir}`);
    }
  }

  const executions = await Promise.allSettled(
    releaseSteps.map((step) => Promise.resolve().then(() => runner(step))),
  );

  let publishOutput = '';
  for (const [index, execution] of executions.entries()) {
    const step = releaseSteps[index];
    if (execution.status === 'rejected') {
      throw execution.reason;
    }
    assertStepSucceeded(step, execution.value);
    if (step.id === 'publish') {
      publishOutput = `${execution.value.stdout}\n${execution.value.stderr}`;
    }
  }

  const publish = parsePublishOutput(publishOutput);
  const verificationRoot = mkdtempSync(path.join(tmpdir(), 'iris-stack-site-verify-'));
  const verificationData = path.join(verificationRoot, 'data');
  const verificationOutput = path.join(verificationRoot, 'retrieved');
  const verificationStep = {
    id: 'verify-publish',
    label: `Verify published Hashtree tree ${publish.nhash}`,
    command: resolveHtreeCommand(
      '--data-dir',
      verificationData,
      'get',
      publish.nhash,
      '--output',
      verificationOutput,
    ),
    cwd: verificationRoot,
    env: { HTREE_CONFIG_DIR: path.join(verificationRoot, 'config') },
  };
  try {
    const result = await runner(verificationStep);
    assertStepSucceeded(verificationStep, result);
    (hooks.verifyRetrievedTree ?? assertTreesByteEqual)(plan.distDir, verificationOutput);
  } finally {
    rmSync(verificationRoot, { recursive: true, force: true });
  }

  return {
    profile: plan.profile,
    treeName: options.treeName,
    publish,
    verified: true,
    workerName: options.skipCloudflare ? null : options.workerName,
    routes: options.skipCloudflare ? [] : options.routes ?? [],
    domains: options.skipCloudflare ? [] : options.domains ?? [],
  };
}

export function usage() {
  return `Usage: node ./scripts/release-site.mjs [options]

Build and test one portable dist, then publish that exact directory to Hashtree
and deploy it to Cloudflare Worker Static Assets.

Options:
  --worker-name <name>    Cloudflare Worker service name
  --tree <name>           Hashtree mutable tree name
  --domain <hostname>     Worker custom domain (default: stack.iris.to)
  --route <pattern>       Worker route override
  --compatibility-date    Worker compatibility date override
  --skip-cloudflare       publish to Hashtree only
  --dry-run               print the release plan without running it

Environment:
  ${releaseProfile.workerNameEnv}   Worker name override
  CF_WORKER_COMPATIBILITY_DATE   Worker compatibility date override
  HTREE_BIN   explicit htree binary
`;
}

function printSummary(result) {
  console.log(`\n${releaseProfile.appName} release complete.`);
  console.log(`Hashtree immutable URL: htree://${result.publish.nhash}/index.html`);
  console.log(`Hashtree mutable URL: htree://${result.publish.publishedRef}`);
  if (result.workerName) {
    console.log(`Worker service: ${result.workerName}`);
  }
  for (const route of result.routes) {
    console.log(`Worker route: ${route}`);
  }
  for (const domain of result.domains) {
    console.log(`Worker custom domain: ${domain}`);
  }
  console.log(`Tree name: ${result.treeName}`);
}

function isMainModule() {
  return Boolean(process.argv[1] && path.resolve(process.argv[1]) === __filename);
}

if (isMainModule()) {
  const main = async () => {
    const parsed = parseArgs(process.argv.slice(2));
    if (parsed.help) {
      console.log(usage());
      return;
    }
    const result = await runRelease(parsed);
    if (result.dryRun) {
      for (const step of result.steps) {
        console.log(`${step.label}: ${step.command.join(' ')} (cwd: ${step.cwd})`);
      }
      return;
    }
    printSummary(result);
  };

  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
