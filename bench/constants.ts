import { homedir, platform } from 'node:os';
import { join } from 'node:path';

function getCacheDir(appName: string): string {
  switch (platform()) {
    case 'win32':
      return join(process.env.LOCALAPPDATA ?? join(homedir(), 'AppData', 'Local'), appName, 'Cache');
    case 'darwin':
      return join(homedir(), 'Library', 'Caches', appName);
    default:
      return join(process.env.XDG_CACHE_HOME ?? join(homedir(), '.cache'), appName);
  }
}

export const BENCH_DIR = process.env.BENCH_DIR || join(getCacheDir('walkrs'), 'datasets');

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
  // { name: '1m', fileCount: 1_000_000 },
  // { name: '10m', fileCount: 10_000_000 },
];
