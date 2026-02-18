#!/usr/bin/env node
import { BENCH_DIR, DATASETS, DatasetConfig } from 'bench/constants';
import { promises as fs } from 'node:fs';
import { join } from 'node:path';

async function createDataset(config: DatasetConfig): Promise<void> {
  const datasetPath = join(BENCH_DIR, config.name);

  console.log(`Creating dataset: ${config.name} (${config.fileCount.toLocaleString()} files)...`);

  await fs.mkdir(datasetPath, { recursive: true });

  // For very large datasets, use a hierarchical structure to avoid inode limits
  const filesPerDir = Math.min(1000, Math.max(10, Math.floor(Math.sqrt(config.fileCount))));
  const dirsNeeded = Math.ceil(config.fileCount / filesPerDir);

  // Create all subdirectories in parallel
  const dirPromises = [];
  for (let dirIdx = 0; dirIdx < dirsNeeded; dirIdx++) {
    const subDir = join(datasetPath, `dir_${String(dirIdx).padStart(6, '0')}`);
    dirPromises.push(fs.mkdir(subDir, { recursive: true }));
  }
  await Promise.all(dirPromises);

  // Create file write operations in batches for better performance
  const BATCH_SIZE = 10_000; // Process 10k files at a time to avoid overwhelming the system
  const EXTENSIONS = ['.txt', '.jpg', '.tif', '.dng', '.dat', '.xyz'];
  let fileCounter = 0;

  for (let batchStart = 0; batchStart < config.fileCount; batchStart += BATCH_SIZE) {
    const batchEnd = Math.min(batchStart + BATCH_SIZE, config.fileCount);
    const filePromises = [];

    for (let idx = batchStart; idx < batchEnd; idx++) {
      const dirIdx = Math.floor(idx / filesPerDir);
      const subDir = join(datasetPath, `dir_${String(dirIdx).padStart(6, '0')}`);
      const ext = EXTENSIONS[idx % EXTENSIONS.length];
      const fileName = join(subDir, `file_${String(idx).padStart(9, '0')}${ext}`);
      filePromises.push(fs.writeFile(fileName, ''));
    }

    await Promise.all(filePromises);
    fileCounter += filePromises.length;

    // Progress indicator for large datasets
    process.stdout.write(`\r  Progress: ${fileCounter.toLocaleString()} / ${config.fileCount.toLocaleString()}`);
  }

  console.log(`\n  ✓ Dataset created: ${config.fileCount.toLocaleString()} files in ${dirsNeeded} directories`);
}

async function main(): Promise<void> {
  console.log('Walkrs Benchmark Dataset Generator\n');

  await fs.mkdir(BENCH_DIR, { recursive: true });

  const args = process.argv.slice(2);
  let datasetsToCreate = DATASETS;

  if (args.length > 0) {
    datasetsToCreate = DATASETS.filter((d) => args.includes(d.name));
    if (datasetsToCreate.length === 0) {
      throw new Error(`No matching datasets found for arguments: ${args.join(', ')}`);
    }
  }

  console.log(`Creating ${datasetsToCreate.length} dataset(s) in ${BENCH_DIR}\n`);

  for (const config of datasetsToCreate) {
    try {
      await createDataset(config);
    } catch (error) {
      console.error(`✗ Error creating dataset ${config.name}:`, error);
    }
  }

  console.log(`\nDatasets created in: ${BENCH_DIR}`);
}

main().catch(console.error);
