#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { walk } from '@immich/walkrs';

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
  const median = sorted.length % 2 === 0
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

async function runBenchmark(
  datasetPath: string,
  iterations: number,
  isBenchmarkMode: boolean
): Promise<BenchStats> {
  // Warmup run (not included in stats)
  console.log('Warming up...');
  await walk({
    paths: [datasetPath],
    benchmark: isBenchmarkMode,
  });

  // Benchmark runs
  const times: number[] = [];
  console.log('Running benchmark...');

  for (let i = 0; i < iterations; i++) {
    const startTime = performance.now();
    await walk({
      paths: [datasetPath],
      benchmark: isBenchmarkMode,
    });
    const duration = performance.now() - startTime;

    times.push(duration);
    const percent = Math.round(((i + 1) / iterations) * 100);
    process.stdout.write(`\r  ${percent.toString().padStart(3)}% (Run ${(i + 1).toString().padStart(3)}/${iterations})`);
  }

  console.log('');

  return calculateStats(times);
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const datasetName = args[0];
  const iterations = Number.parseInt(args[1] || '10', 10);

  if (!datasetName) {
    console.error('Usage: node bench/bench.ts <dataset folder> [iterations]');
    console.error('');
    console.error('Examples:');
    console.error('  node bench/bench.ts 1k');
    console.error('  node bench/bench.ts 10k 20');
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

  console.log('');
  console.log('Full Stack Results:');
  console.log(`. Mean:     ${withSerializationStats.mean.toFixed(2)} ms`);
  console.log(`. Median:   ${withSerializationStats.median.toFixed(2)} ms`);
  console.log(`. Std Dev:  ${withSerializationStats.stdDev.toFixed(2)} ms`);
  console.log(`. Min:      ${withSerializationStats.min.toFixed(2)} ms`);
  console.log(`. Max:      ${withSerializationStats.max.toFixed(2)} ms`);
  console.log(`. Range:    ${(withSerializationStats.max - withSerializationStats.min).toFixed(2)} ms`);

  // Run 2: Pure Rust without serialization
  console.log('');
  console.log('='.repeat(60));
  console.log('Pure Rust (no serialization overhead)');
  console.log('='.repeat(60));
  const pureRustStats = await runBenchmark(datasetPath, iterations, true);

  console.log('');
  console.log('Pure Rust Results:');
  console.log(`. Mean:     ${pureRustStats.mean.toFixed(2)} ms`);
  console.log(`. Median:   ${pureRustStats.median.toFixed(2)} ms`);
  console.log(`. Std Dev:  ${pureRustStats.stdDev.toFixed(2)} ms`);
  console.log(`. Min:      ${pureRustStats.min.toFixed(2)} ms`);
  console.log(`. Max:      ${pureRustStats.max.toFixed(2)} ms`);
  console.log(`. Range:    ${(pureRustStats.max - pureRustStats.min).toFixed(2)} ms`);

  // Comparison
  console.log('');
  console.log('='.repeat(60));
  console.log('Comparison: Serialization Overhead');
  console.log('='.repeat(60));
  const overheadMs = withSerializationStats.mean - pureRustStats.mean;
  console.log(`   Full Stack (Mean): ${withSerializationStats.mean.toFixed(2)} ms`);
  console.log(`   Pure Rust (Mean):          ${pureRustStats.mean.toFixed(2)} ms`);
  console.log(`   Overhead:                  ${overheadMs.toFixed(2)} ms (${((overheadMs / pureRustStats.mean) * 100).toFixed(1)}%)`);
}

main().catch((error) => {
  console.error('Benchmark error:', error);
  process.exit(1);
});