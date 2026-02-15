#!/usr/bin/env node
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const BENCH_DIR = path.join(__dirname, 'datasets');

interface DatasetConfig {
  name: string;
  fileCount: number;
}

const DATASETS: DatasetConfig[] = [
  { name: '10', fileCount: 10 },
  { name: '100', fileCount: 100 },
  { name: '1k', fileCount: 1000 },
  { name: '10k', fileCount: 10_000 },
  { name: '100k', fileCount: 100_000 },
  { name: '1m', fileCount: 1_000_000 },
  { name: '10m', fileCount: 10_000_000 },
  { name: '100m', fileCount: 100_000_000 },
];

async function createDataset(config: DatasetConfig): Promise<void> {
  const datasetPath = path.join(BENCH_DIR, config.name);
  
  console.log(`Creating dataset: ${config.name} (${config.fileCount.toLocaleString()} files)...`);
  
  try {
    await fs.access(datasetPath);
    // Directory exists, check if it's empty
    const entries = await fs.readdir(datasetPath);
    if (entries.length > 0) {
      throw new Error(`Dataset directory already exists and is not empty: ${datasetPath}. Please clear it before continuing setup.`);
    }
  } catch (error) {
    // If error is not about non-empty directory, it means directory doesn't exist
    if (error instanceof Error && error.message.includes('not empty')) {
      throw error;
    }
    // Directory doesn't exist, which is fine
  }
  
  await fs.mkdir(datasetPath, { recursive: true });
  
  // For very large datasets, use a hierarchical structure to avoid inode limits
  const filesPerDir = Math.min(1000, Math.max(10, Math.floor(Math.sqrt(config.fileCount))));
  const dirsNeeded = Math.ceil(config.fileCount / filesPerDir);
  
  // Create all subdirectories in parallel
  const dirPromises = [];
  for (let dirIdx = 0; dirIdx < dirsNeeded; dirIdx++) {
    const subDir = path.join(datasetPath, `dir_${String(dirIdx).padStart(6, '0')}`);
    dirPromises.push(fs.mkdir(subDir, { recursive: true }));
  }
  await Promise.all(dirPromises);
  
  // Create file write operations in batches for better performance
  const BATCH_SIZE = 10_000; // Process 10k files at a time to avoid overwhelming the system
  let fileCounter = 0;
  
  for (let batchStart = 0; batchStart < config.fileCount; batchStart += BATCH_SIZE) {
    const batchEnd = Math.min(batchStart + BATCH_SIZE, config.fileCount);
    const filePromises = [];
    
    for (let idx = batchStart; idx < batchEnd; idx++) {
      const dirIdx = Math.floor(idx / filesPerDir);
      const subDir = path.join(datasetPath, `dir_${String(dirIdx).padStart(6, '0')}`);
      const fileName = path.join(subDir, `file_${String(idx).padStart(9, '0')}.txt`);
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
  console.log('🚀 Walkrs Benchmark Dataset Generator\n');
  
  const args = process.argv.slice(2);
  let datasetsToCreate = DATASETS;
  
  if (args.length > 0) {
    datasetsToCreate = DATASETS.filter(d => args.includes(d.name));
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
  
  console.log('\n✅ Setup complete!');
  console.log(`\nDatasets created in: ${BENCH_DIR}`);
  console.log('Run benchmarks with: pnpm bench:full or pnpm bench:rust');
}

main().catch(console.error);