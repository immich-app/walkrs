import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export const BENCH_DIR = path.join(__dirname, 'datasets');

export interface DatasetConfig {
  name: string;
  fileCount: number;
}

export const DATASETS: DatasetConfig[] = [
  { name: '10', fileCount: 10 },
  { name: '100', fileCount: 100 },
  { name: '1k', fileCount: 1000 },
  { name: '10k', fileCount: 10_000 },
  { name: '100k', fileCount: 100_000 },
  { name: '1m', fileCount: 1_000_000 },
  { name: '10m', fileCount: 10_000_000 },
];
