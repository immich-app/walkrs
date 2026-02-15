#!/usr/bin/env node
import { walk } from '@immich/walkrs';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { DATASETS , DatasetConfig} from 'bench/constants';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const BENCH_DIR = path.join(__dirname, 'datasets');

interface BenchStats {
  iterations: number;
  times: number[];
  mean: number;
  median: number;
  stdDev: number;
  min: number;
  max: number;
}

function calculateStats(times: number[]): BenchStats {
  const sorted = [...times].toSorted((a, b) => a - b);
  const mean = times.reduce((a, b) => a + b, 0) / times.length;
  const variance = times.reduce((a, b) => a + Math.pow(b - mean, 2), 0) / times.length;
  const stdDev = Math.sqrt(variance);
  const median =
    sorted.length % 2 === 0
      ? (sorted[sorted.length / 2 - 1] + sorted[sorted.length / 2]) / 2
      : sorted[Math.floor(sorted.length / 2)];

  return {
    iterations: times.length,
    times,
    mean,
    median,
    stdDev,
    min: sorted[0],
    max: sorted.at(-1)!,
  };
}

function printBenchmarkResults(title: string, stats: BenchStats): void {
  const meanText = stats.mean.toFixed(1).padStart(6);
  const stdDevText = stats.stdDev.toFixed(1).padStart(6);
  const minText = stats.min.toFixed(1).padStart(6);
  const maxText = stats.max.toFixed(1).padStart(6);

  console.log('');
  console.log(title);
  console.log(`Time (mean ± σ):     ${meanText} ms ± ${stdDevText} ms`);
  console.log(`Range (min … max):   ${minText} ms … ${maxText} ms    ${stats.iterations} runs`);
}

async function drainWalk(datasetPath: string, isBenchmarkMode: boolean): Promise<void> {
  for await (const _entry of walk({
    paths: [datasetPath],
    benchmark: isBenchmarkMode,
    // eslint-disable-next-line no-empty
  })) {
  }
}

async function runBenchmark(datasetPath: string, iterations: number, isBenchmarkMode: boolean): Promise<BenchStats> {
  console.log('Warming up...');
  await drainWalk(datasetPath, isBenchmarkMode);

  const times: number[] = [];
  console.log('Running benchmark...');

  for (let i = 0; i < iterations; i++) {
    const startTime = performance.now();
    await drainWalk(datasetPath, isBenchmarkMode);
    const duration = performance.now() - startTime;

    times.push(duration);
    const percent = Math.round(((i + 1) / iterations) * 100);
    process.stdout.write(
      `\r  ${percent.toString().padStart(3)}% (Run ${(i + 1).toString().padStart(3)}/${iterations})`,
    );
  }

  console.log('');

  return calculateStats(times);
}

function parseArgs(args: string[]): { datasetName?: string; iterations: number } {
  let datasetName: string | undefined;
  let iterations = 10;

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--dataset' || arg === '-d') {
      datasetName = args[i + 1];
      i++;
      continue;
    }

    if (arg.startsWith('--dataset=')) {
      datasetName = arg.slice('--dataset='.length);
      continue;
    }

    if (arg === '--iterations' || arg === '-i') {
      const value = args[i + 1];
      iterations = Number.parseInt(value ?? '', 10);
      i++;
      continue;
    }

    if (arg.startsWith('--iterations=')) {
      iterations = Number.parseInt(arg.slice('--iterations='.length), 10);
      continue;
    }

    console.error(`Unknown argument: ${arg}`);
    process.exit(1);
  }

  return { datasetName, iterations };
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const { datasetName, iterations } = parseArgs(args);


  if (!datasetName) {
    console.error('Usage: node bench/bench.ts --dataset <folder> [--iterations <count>]');
    console.error('');
    console.error('Examples:');
    console.error('  node bench/bench.ts --dataset 1k');
    console.error('  node bench/bench.ts --dataset 10k --iterations 20');
    process.exit(1);
  }

  if (iterations < 1 || iterations > 1000) {
    console.error('Error: iterations must be between 1 and 1000');
    process.exit(1);
  }

  const datasetPath = path.join(BENCH_DIR, datasetName);

  if (!fs.existsSync(datasetPath)) {
    console.error(`Error: Dataset not found: ${datasetPath}`);
    console.error('Use the setup script to create sets: node bench/setup.ts <dataset> to create it');
    process.exit(1);
  }

  console.log(`Benchmarking file walk on dataset: ${datasetName} and ${iterations} iterations`);
  console.log('');

  console.log('='.repeat(60));
  console.log('With Serialization (rust and node performance)');
  console.log('='.repeat(60));
  const withSerializationStats = await runBenchmark(datasetPath, iterations, false);

  printBenchmarkResults('Full Stack Results:', withSerializationStats);

  // Run 2: Pure Rust without serialization
  console.log('');
  console.log('='.repeat(60));
  console.log('Pure Rust (no serialization overhead)');
  console.log('='.repeat(60));
  const pureRustStats = await runBenchmark(datasetPath, iterations, true);

  printBenchmarkResults('Pure Rust Results:', pureRustStats);

  // Comparison
  console.log('');
  console.log('='.repeat(60));
  console.log('Comparison: Serialization Overhead');
  console.log('='.repeat(60));
  const overheadMs = withSerializationStats.mean - pureRustStats.mean;
  console.log(`   Full Stack (Mean): ${withSerializationStats.mean.toFixed(2)} ms`);
  console.log(`   Pure Rust (Mean):          ${pureRustStats.mean.toFixed(2)} ms`);
  console.log(
    `   Overhead:                  ${overheadMs.toFixed(2)} ms (${((overheadMs / pureRustStats.mean) * 100).toFixed(1)}%)`,
  );
}

main().catch(console.error);
