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
const files = await walk({
  root_paths: ['/path/to/scan'],
});

// Advanced usage with filtering
const photos = await walk({
  root_paths: ['/photos', '/backup/photos'],
  supported_extensions: ['.jpg', '.png', '.heic', '.webp'],
  exclusion_patterns: ['**/.stfolder/**'],
  include_hidden: false,
});
```

## Performance

walkrs is designed to handle massive directory trees efficiently. It is greatly affected by multithreading: In benchmarks we have scanned 11M files in under 30 seconds over NFS on a machine with 32 CPU threads available. When restricting walkrs to a single thread, the time for the same task goes up to 208 seconds. Compare this with the single-threaded fast-glob which uses 360 seconds for the same task.
