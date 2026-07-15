import { rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..');

function takeValue(args, option) {
  const value = args.shift();
  if (!value || value.startsWith('--')) {
    throw new Error(`Missing value for ${option}`);
  }
  return value;
}

export function parseArgs(argv) {
  const options = {
    routes: [],
    domains: [],
    wranglerVersion: '4',
  };
  const args = [...argv];

  while (args.length > 0) {
    const arg = args.shift();
    if (arg === '--script') {
      options.script = takeValue(args, arg);
      continue;
    }
    if (arg === '--assets') {
      options.assets = takeValue(args, arg);
      continue;
    }
    if (arg === '--name') {
      options.name = takeValue(args, arg);
      continue;
    }
    if (arg === '--compatibility-date') {
      options.compatibilityDate = takeValue(args, arg);
      continue;
    }
    if (arg === '--wrangler-version') {
      options.wranglerVersion = takeValue(args, arg);
      continue;
    }
    if (arg === '--route') {
      options.routes.push(takeValue(args, arg));
      continue;
    }
    if (arg === '--domain') {
      options.domains.push(takeValue(args, arg));
      continue;
    }
    throw new Error(`Unknown argument: ${arg}`);
  }

  for (const [key, option] of [
    ['script', '--script'],
    ['assets', '--assets'],
    ['name', '--name'],
    ['compatibilityDate', '--compatibility-date'],
  ]) {
    if (!options[key]) {
      throw new Error(`Missing required option: ${option}`);
    }
  }

  return options;
}

export function configFor(options) {
  return {
    name: options.name,
    compatibility_date: options.compatibilityDate,
    main: path.relative(repoRoot, path.resolve(repoRoot, options.script)),
    assets: {
      directory: path.relative(repoRoot, path.resolve(repoRoot, options.assets)),
      binding: 'ASSETS',
      run_worker_first: true,
    },
  };
}

export function wranglerCommandFor(options, configPath) {
  const command = [
    `wrangler@${options.wranglerVersion}`,
    'deploy',
    '--config',
    configPath,
    '--keep-vars',
  ];
  for (const route of options.routes) {
    command.push('--route', route);
  }
  for (const domain of options.domains) {
    command.push('--domain', domain);
  }
  return ['npx', ...command];
}

function run(command, args, cwd) {
  console.log(`$ ${[command, ...args].join(' ')}`);
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, stdio: 'inherit' });
    child.on('error', reject);
    child.on('close', (code, signal) => {
      if (signal) {
        reject(new Error(`Cloudflare deploy interrupted by ${signal}`));
        return;
      }
      resolve(code ?? 1);
    });
  });
}

export async function deployWorkerAssets(options, runner = run) {
  const configPath = path.join(
    repoRoot,
    `.wrangler-assets-${process.pid}-${Date.now()}.json`,
  );

  try {
    await writeFile(configPath, `${JSON.stringify(configFor(options), null, 2)}\n`);
    const [command, ...args] = wranglerCommandFor(options, configPath);
    const status = await runner(command, args, repoRoot);
    if (status !== 0) {
      throw new Error(`Cloudflare Worker deploy failed with exit code ${status}`);
    }
  } finally {
    await rm(configPath, { force: true });
  }
}

function isMainModule() {
  return Boolean(process.argv[1] && path.resolve(process.argv[1]) === __filename);
}

if (isMainModule()) {
  deployWorkerAssets(parseArgs(process.argv.slice(2))).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
