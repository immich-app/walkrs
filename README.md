# @immich/walkrs

High-performance file tree walker for Node.js, built with Rust and the battle-tested [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore) crate from ripgrep.

## Background

This project grew out of the need for fast and reliable external library scanning in [Immich](https://github.com/immich-app/immich). Immich needed to scan very large photo libraries efficiently, often containing hundreds of thousands of files across complex directory structures.

By leveraging Rust's performance and the same ignore logic used in ripgrep (one of the fastest file search tools available), walkrs delivers exceptional speed and reliability for file tree traversal.

## Installation

```bash
pnpm add @immich/walkrs
```

## Usage

```typescript
import { walk } from '@immich/walkrs';

// Simple usage - walk a directory
const files: string[] = [];
for await (const batch of walk({ paths: ['/path/to/scan'] })) {
  files.push(...JSON.parse(batch));
}

// Advanced usage with filtering
const photos: string[] = [];
for await (const batch of walk({
  paths: ['/photos', '/backup/photos'],
  extensions: ['.jpg', '.png', '.heic', '.webp'],
  exclusionPatterns: ['**/.stfolder/**'],
  includeHidden: false,
})) {
  photos.push(...JSON.parse(batch));
}
```

## Performance

walkrs is designed to handle massive directory trees efficiently. It is greatly affected by multithreading: In benchmarks we have scanned 11M files in under 30 seconds over NFS on a machine with 32 CPU threads available. When restricting walkrs to a single thread, the time for the same task goes up to 208 seconds. Compare this with the single-threaded fast-glob which uses 360 seconds for the same task.

## Benchmarking

Since performance is critical, we provide dedicated benchmark scripts.

### Setup

Before running benchmarks, you need to create benchmark datasets. This is a one-time setup that generates test directories with various file counts. **Note: This can take several minutes to complete depending on your system.**

```bash
pnpm run bench:setup
```

This creates datasets in the `bench/datasets/` directory:

- `10` - 10 files
- `100` - 100 files
- `1k` - 1,000 files
- `10k` - 10,000 files
- `100k` - 100,000 files
- `1m` - 1,000,000 files
- `10m` - 10,000,000 files

### Running Benchmarks

Run benchmarks against any dataset:

```bash
# Run with default settings on all datasets
pnpm run ts:bench

# Run on a specific dataset
pnpm run ts:bench 1m

# Run multiple datasets
pnpm run ts:bench 100 10k 1m
```
