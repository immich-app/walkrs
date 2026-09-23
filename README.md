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
  files.push(...batch.files);
}

// Advanced usage with filtering
const photos: string[] = [];
for await (const batch of walk({
  paths: ['/photos', '/backup/photos'],
  extensions: ['.jpg', '.png', '.heic', '.webp'],
  exclusionPatterns: ['**/.stfolder/**'],
  includeHidden: false,
})) {
  photos.push(...batch.files);
}

// Include file sizes and modification times
for await (const batch of walk({ paths: ['/photos'], includeMetadata: true })) {
  if (batch.size !== null && batch.modified !== null) {
    for (let i = 0; i < batch.files.length; i++) {
      // Path, size in bytes, and modification time in Unix milliseconds
      console.log(batch.files[i], batch.size[i], batch.modified[i]);
    }
  }
  for (const error of batch.errors) {
    console.error(error.path, error.message);
  }
}
```

Every batch has `{ files, size, modified, errors }`. By default, `size` and `modified` are `null`, and the walker does not request file metadata. Set `includeMetadata: true` to get parallel integer arrays: `size[i]` is the byte size of `files[i]`, and `modified[i]` is its modification time in Unix milliseconds, truncated toward zero. Creation time is not collected.

The metadata arrays always have the same length and order as `files`. If metadata cannot be read, that file is omitted from all three arrays and an entry with its path is added to `errors`. Values outside JavaScript's safe integer range are also reported as errors. A batch containing only errors has empty metadata arrays when metadata is enabled. An empty walk produces no batches. File order is unspecified, and metadata is a snapshot that can change after the file is visited.

## Performance

walkrs is designed to handle massive directory trees efficiently. It is greatly affected by multithreading: In benchmarks we have scanned 11M files in under 30 seconds over NFS on a machine with 32 CPU threads available. When restricting walkrs to a single thread, the time for the same task goes up to 208 seconds. Compare this with the single-threaded fast-glob based on nodejs which uses 360 seconds for the same task.

## Benchmarking

Since performance is critical, we provide dedicated benchmark scripts.

### Setup

Before running benchmarks, you need to create benchmark datasets. This is a one-time setup that generates test directories with various file counts. **Note: This can take several minutes to complete depending on your system.**

```bash
pnpm run bench:setup
```

This creates datasets in the platform's cache directory, or the directory specified by `BENCH_DIR`:

- `10` - 10 files
- `100` - 100 files
- `1k` - 1,000 files
- `10k` - 10,000 files
- `100k` - 100,000 files
- `1m` - 1,000,000 files
- `10m` - 10,000,000 files

### Running Benchmarks

Run benchmarks against any dataset:

The benchmark compares paths-only and metadata-enabled walks at each thread count, along with filtering scenarios.

```bash
# Run with default settings on all datasets
pnpm run ts:bench

# Run on a specific dataset
pnpm run ts:bench 1m

# Run multiple datasets
pnpm run ts:bench 100 10k 1m
```
