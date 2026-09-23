#!/usr/bin/env node
import { walk } from '@immich/walkrs';
import { BENCH_DIR, DATASETS } from 'bench/constants.js';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Bench } from 'tinybench';

const EXCLUSION_PATTERNS = ['**/*6*/**'];
const EXTENSIONS = ['jpg'];

interface BenchmarkOptions {
  exclusionPatterns?: string[];
  extensions?: string[];
  threads?: number;
  includeMetadata?: boolean;
}

async function run(datasetPath: string, benchmarkOptions?: BenchmarkOptions): Promise<number> {
  const walkOptions = {
    paths: [datasetPath],
    includeMetadata: benchmarkOptions?.includeMetadata ?? false,
    ...(benchmarkOptions?.exclusionPatterns && { exclusionPatterns: benchmarkOptions.exclusionPatterns }),
    ...(benchmarkOptions?.extensions && { extensions: benchmarkOptions.extensions }),
    ...(benchmarkOptions?.threads && { threads: benchmarkOptions.threads }),
  };

  let fileCount = 0;
  for await (const batch of walk(walkOptions)) {
    if (batch.errors.length > 0) {
      throw new Error(`Walk encountered errors: ${batch.errors.map((e) => e.message).join(', ')}`);
    }
    fileCount += batch.files.length;
    if (
      walkOptions.includeMetadata &&
      (batch.size?.length !== batch.files.length || batch.modified?.length !== batch.files.length)
    ) {
      throw new Error('Metadata columns are not aligned with paths');
    }
  }

  return fileCount;
}

async function main(): Promise<void> {
  const specifiedDatasets: string[] = [];

  for (const arg of process.argv.slice(2)) {
    if (arg.startsWith('-')) {
      console.error(`Unknown argument: ${arg}`);
      process.exit(1);
    }

    specifiedDatasets.push(arg);
  }

  const defaultDatasets = DATASETS.filter((d) => d.default).map((d) => d.name);

  const datasets = specifiedDatasets.length > 0 ? specifiedDatasets : defaultDatasets;

  console.log(`Benchmarking file walk on datasets: ${datasets.join(', ')}`);

  const maxThreads = os.cpus().length;
  const threadCounts: number[] = [1];
  if (maxThreads >= 2) {
    threadCounts.push(2);
  }
  if (maxThreads > 2) {
    threadCounts.push(0); // 0 for auto
  }

  const bench = new Bench({
    time: 20,
    warmupIterations: 1,
  });

  for (const dataset of datasets) {
    const datasetPath = path.join(BENCH_DIR, dataset);

    if (!fs.existsSync(datasetPath)) {
      console.error(`Error: Dataset not found: ${datasetPath}`);
      console.error('Use the setup script to create sets');
      process.exit(1);
    }

    const fileCount = await run(datasetPath);
    const exclusionCount = await run(datasetPath, { exclusionPatterns: EXCLUSION_PATTERNS });
    const extensionCount = await run(datasetPath, { extensions: EXTENSIONS });
    const combinedCount = await run(datasetPath, {
      exclusionPatterns: EXCLUSION_PATTERNS,
      extensions: EXTENSIONS,
    });

    console.log(`Dataset: ${dataset}`);
    console.log(`  Total files: ${fileCount}`);
    console.log(`  After exclusions: ${exclusionCount}`);
    console.log(`  After extensions: ${extensionCount}`);
    console.log(`  After exclusions + extensions: ${combinedCount}`);

    for (const threads of threadCounts) {
      // Baseline - no options
      bench.add(`${dataset}, threads: ${threads}`, () => run(datasetPath, { threads }));

      bench.add(`${dataset} (metadata), threads: ${threads}`, () =>
        run(datasetPath, { threads, includeMetadata: true }),
      );

      // Add an exclusion pattern
      bench.add(`${dataset} (exclusions), threads: ${threads}`, () =>
        run(datasetPath, { exclusionPatterns: EXCLUSION_PATTERNS, threads }),
      );

      // Add an extension filter
      bench.add(`${dataset} (extensions), threads: ${threads}`, () =>
        run(datasetPath, { extensions: EXTENSIONS, threads }),
      );

      // Add both exclusions and extensions
      bench.add(`${dataset} (exclusions + extensions), threads: ${threads}`, () =>
        run(datasetPath, {
          exclusionPatterns: EXCLUSION_PATTERNS,
          extensions: EXTENSIONS,
          threads,
        }),
      );
    }
  }

  await bench.run();

  console.table(bench.table());
}

main().catch(console.error);
