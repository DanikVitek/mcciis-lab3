#!/usr/bin/env -S deno run --allow-run --allow-read --allow-write
/**
 * Collects exact per-crate unit, integration, and doc-test counts from Cargo
 * and writes them to `report/test-stats.json` for native import in Typst.
 */

interface CargoTarget {
  name: string;
  kind: string[];
  doctest: boolean;
}

interface CargoPackage {
  id: string;
  name: string;
  targets: CargoTarget[];
}

interface CargoMetadata {
  packages: CargoPackage[];
  workspace_members: string[];
}

interface SuiteCounts {
  passed: number;
  failed: number;
  ignored: number;
}

interface CrateTestStats {
  unit: number;
  doc: number;
  integration: number;
  passed: number;
  failed: number;
  ignored: number;
  total: number;
}

interface TestStatsReport {
  crates: Record<string, CrateTestStats>;
  totals: CrateTestStats;
}

const SCRIPT_DIR = new URL('.', import.meta.url);
const ROOT_DIR = new URL('..', SCRIPT_DIR);
const OUTPUT_PATH = new URL('./test-stats.json', SCRIPT_DIR);

const TEST_RESULT_RE =
  /^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; \d+ filtered out;/gm;

function urlToPath(url: URL): string {
  return decodeURIComponent(url.pathname);
}

async function runCargo(args: string[]): Promise<{ stdout: string; stderr: string }> {
  const cmd = new Deno.Command('cargo', {
    args,
    cwd: urlToPath(ROOT_DIR),
    stdout: 'piped',
    stderr: 'piped',
  });
  const out = await cmd.output();
  const stdout = new TextDecoder().decode(out.stdout);
  const stderr = new TextDecoder().decode(out.stderr);
  if (!out.success) {
    throw new Error(
      `cargo ${args.join(' ')} failed with exit code ${out.code}\n${stdout}\n${stderr}`,
    );
  }
  return { stdout, stderr };
}

function parseTestResults(stdout: string): SuiteCounts {
  let passed = 0;
  let failed = 0;
  let ignored = 0;
  for (const match of stdout.matchAll(TEST_RESULT_RE)) {
    passed += Number(match[1]);
    failed += Number(match[2]);
    ignored += Number(match[3]);
  }
  return { passed, failed, ignored };
}

function emptyStats(): CrateTestStats {
  return {
    unit: 0,
    doc: 0,
    integration: 0,
    passed: 0,
    failed: 0,
    ignored: 0,
    total: 0,
  };
}

function sumSuite(counts: SuiteCounts): number {
  return counts.passed + counts.failed + counts.ignored;
}

async function main(): Promise<void> {
  const { stdout: metaJson } = await runCargo([
    'metadata',
    '--no-deps',
    '--format-version',
    '1',
  ]);
  const metadata = JSON.parse(metaJson) as CargoMetadata;
  const memberIds = new Set(metadata.workspace_members);
  const packages = metadata.packages
    .filter((pkg) => memberIds.has(pkg.id))
    .sort((a, b) => a.name.localeCompare(b.name));

  // Compile all workspace test targets once in parallel.
  await runCargo(['test', '--workspace', '--no-run']);

  const crates: Record<string, CrateTestStats> = {};
  const totals = emptyStats();

  for (const pkg of packages) {
    const stats = emptyStats();
    const hasLib = pkg.targets.some((t) => t.kind.includes('lib'));
    const hasBin = pkg.targets.some((t) => t.kind.includes('bin'));
    const hasDoctests = pkg.targets.some((t) => t.kind.includes('lib') && t.doctest);
    const integrationTargets = pkg.targets
      .filter((t) => t.kind.includes('test'))
      .map((t) => t.name);

    if (hasLib || hasBin) {
      const unitArgs = ['test', '-p', pkg.name];
      if (hasLib) unitArgs.push('--lib');
      if (hasBin) unitArgs.push('--bins');
      unitArgs.push('--', '--format', 'terse');

      const { stdout } = await runCargo(unitArgs);
      const res = parseTestResults(stdout);
      stats.unit = sumSuite(res);
      stats.passed += res.passed;
      stats.failed += res.failed;
      stats.ignored += res.ignored;
    }

    if (integrationTargets.length > 0) {
      const intArgs = ['test', '-p', pkg.name];
      for (const targetName of integrationTargets) {
        intArgs.push('--test', targetName);
      }
      intArgs.push('--', '--format', 'terse');

      const { stdout } = await runCargo(intArgs);
      const res = parseTestResults(stdout);
      stats.integration = sumSuite(res);
      stats.passed += res.passed;
      stats.failed += res.failed;
      stats.ignored += res.ignored;
    }

    if (hasDoctests) {
      const { stdout } = await runCargo([
        'test',
        '-p',
        pkg.name,
        '--doc',
        '--',
        '--format',
        'terse',
      ]);
      const res = parseTestResults(stdout);
      stats.doc = sumSuite(res);
      stats.passed += res.passed;
      stats.failed += res.failed;
      stats.ignored += res.ignored;
    }

    stats.total = stats.unit + stats.doc + stats.integration;
    crates[pkg.name] = stats;

    totals.unit += stats.unit;
    totals.doc += stats.doc;
    totals.integration += stats.integration;
    totals.passed += stats.passed;
    totals.failed += stats.failed;
    totals.ignored += stats.ignored;
    totals.total += stats.total;
  }

  if (totals.failed > 0) {
    throw new Error(`Refusing to write test stats: ${totals.failed} test(s) failed.`);
  }

  const report: TestStatsReport = { crates, totals };
  const outputPath = urlToPath(OUTPUT_PATH);
  await Deno.writeTextFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
  console.log(
    `Wrote ${outputPath}: ${totals.passed}/${totals.total} passed ` +
      `(unit=${totals.unit}, doc=${totals.doc}, integration=${totals.integration})`,
  );
}

if (import.meta.main) {
  await main();
}
