import { walk, type WalkOptions } from '@immich/walkrs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

// Synthetic filesystem-only comparison; database and queue costs are excluded.
// BENCH_DIR can point at HDD/NFS storage. Each invocation owns a temporary tree.
const count = Number(process.argv[2] ?? 10_000);
const threads = Number(process.argv[3] ?? 1);
if (!Number.isSafeInteger(count) || count <= 0 || !Number.isSafeInteger(threads) || threads < 0) {
  throw new Error('Usage: bench:sidecars [positive file count] [nonnegative thread count]');
}
const base = process.env.BENCH_DIR ?? os.tmpdir();
await fs.mkdir(base, { recursive: true });
const directory = await fs.mkdtemp(path.join(base, 'walkrs-sidecar-bench-'));
let baselineWalk: typeof walk | undefined;
if (process.env.WALKRS_BASELINE_MODULE) {
  const baseline = await import(pathToFileURL(process.env.WALKRS_BASELINE_MODULE).href);
  baselineWalk = baseline.walk;
}

async function measure(root: string, mode: 'paths' | 'native' | 'js', newEvery: number, walker = walk) {
  const options: WalkOptions = {
    paths: [root],
    extensions: ['jpg'],
    threads,
    includeMetadata: mode === 'native',
    includeSidecars: mode === 'native',
  };
  let files = 0;
  let found = 0;
  let probes = 0;
  let firstBatch = 0;
  const started = performance.now();
  for await (const batch of walker(options)) {
    if (!firstBatch) {
      firstBatch = performance.now() - started;
    }
    if (batch.errors.length > 0) {
      throw new Error(JSON.stringify(batch.errors));
    }
    if (mode === 'native') {
      if (batch.sidecars?.length !== batch.files.length || batch.modified?.length !== batch.files.length) {
        throw new Error('Unaligned scan result');
      }
      found += batch.sidecars.filter((sidecar) => typeof sidecar === 'string').length;
    }
    if (mode === 'js') {
      const selected = batch.files.filter((_, index) => newEvery > 0 && (files + index) % newEvery === 0);
      for (let start = 0; start < selected.length; start += 32) {
        await Promise.all(
          selected.slice(start, start + 32).map(async (filename) => {
            await fs.stat(filename);
            const parsed = path.parse(filename);
            for (const candidate of [`${filename}.xmp`, path.join(parsed.dir, `${parsed.name}.xmp`)]) {
              probes++;
              const readable = await fs.access(candidate, fs.constants.R_OK).then(
                () => true,
                () => false,
              );
              if (readable) {
                found++;
                break;
              }
            }
          }),
        );
      }
    }
    files += batch.files.length;
  }
  return {
    files,
    found,
    jsAccessCalls: probes,
    ms: Math.round(performance.now() - started),
    firstBatchMs: Math.round(firstBatch),
  };
}

try {
  const results = [];
  for (const scenario of ['none', 'clustered', 'spread', 'orphans', 'dense'] as const) {
    const root = path.join(directory, scenario);
    await fs.mkdir(root);
    for (let start = 0; start < count; start += 500) {
      await Promise.all(
        Array.from({ length: Math.min(500, count - start) }, async (_, offset) => {
          const index = start + offset;
          const dirname = path.join(root, String(Math.floor(index / 1000)));
          await fs.mkdir(dirname, { recursive: true });
          const filename = path.join(dirname, `${index}.jpg`);
          await fs.writeFile(filename, '');
          if (
            scenario === 'dense' ||
            (scenario === 'clustered' && index < 10) ||
            (scenario === 'spread' && index % 1000 === 0)
          ) {
            await fs.writeFile(`${filename}.xmp`, '');
          }
          if (scenario === 'orphans' && index % 1000 === 0) {
            await fs.writeFile(path.join(dirname, 'orphan.xmp'), '');
          }
        }),
      );
    }
    if (baselineWalk) {
      results.push({ scenario, mode: 'previous paths', ...(await measure(root, 'paths', 0, baselineWalk)) });
    }
    results.push(
      { scenario, mode: 'current paths', ...(await measure(root, 'paths', 0)) },
      { scenario, mode: 'native import scan', ...(await measure(root, 'native', 0)) },
    );
    for (const [newEvery, label] of [
      [1, '100% new'],
      [100, '1% new'],
      [0, '0% new'],
    ] as const) {
      results.push({
        scenario,
        mode: `JS import (${label})`,
        ...(await measure(root, 'js', newEvery, baselineWalk ?? walk)),
      });
    }
  }
  console.table(results);
  console.log(JSON.stringify({ count, threads, filesystem: base, results }));
} finally {
  await fs.rm(directory, { recursive: true, force: true });
}
