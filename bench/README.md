# Walkrs Benchmarks

This directory contains benchmarks for the walkrs file tree walker with built-in statistical analysis.

## Setup

First, create benchmark datasets:

```bash
# Create small datasets (recommended for initial testing)
pnpm bench:setup:small

# Or create specific datasets
node --import tsx bench/setup.ts 1k 10k

# Or create all datasets (warning: 100m dataset = 100 million files!)
pnpm bench:setup
```

Available datasets: `10`, `100`, `1k`, `10k`, `100k`, `1m`, `10m`, `100m`

## Running Benchmarks

The benchmarks measure the critical walk() function timing and calculate statistics automatically.

```bash
# Benchmark with default 10 iterations
node --import tsx bench/fullstack.ts 1k

# Benchmark with custom number of iterations
node --import tsx bench/fullstack.ts 1k 20

# Compare with Rust implementation (NAPI overhead included)
node --import tsx bench/rustonly.ts 10k 15
```

### Using npm/pnpm scripts

```bash
pnpm bench:full 1k          # Run fullstack benchmark on 1k dataset
pnpm bench:rust 10k         # Run rustonly benchmark on 10k dataset
```

### Output Example

```
🚀 Full-Stack Benchmark (Disk → Rust → TypeScript)
   Dataset: 1k
   Iterations: 10

Warming up...
Running benchmark...
  100% (Run  10/10)

📊 Results (internal walk() timing):
   Mean:     5.23 ms
   Median:   5.15 ms
   Std Dev:  0.34 ms
   Min:      4.89 ms
   Max:      6.12 ms
   Range:    1.23 ms
```

## Benchmark Types

### fullstack.ts
Full-stack benchmark measuring the complete pipeline from disk → Rust → TypeScript. This includes:
- File system I/O
- Rust path walking logic
- NAPI serialization overhead
- TypeScript result processing

### rustonly.ts
Similar to fullstack but focuses on Rust performance during the walk (NAPI overhead is still included since it goes through Node.js).

## Understanding the Timings

- **Mean**: Average time across all runs
- **Median**: Middle value, robust to outliers
- **Std Dev**: Standard deviation showing timing variance
- **Min/Max**: Fastest and slowest runs
- **Range**: Difference between max and min

The measurements exclude Node.js startup overhead and focus on the critical walk() function performance.

## Tips

- **Warmup**: First run is a warmup to populate filesystem caches
- **Iterations**: Use 10-20 runs for reliable statistics (default: 10)
- **System preparation**: Close unnecessary applications for best results
- **Consistency**: Filesystem caches are warmed up, so results are stable

## Cleanup

Remove datasets to free up disk space:

```bash
rm -rf bench/datasets/*
```
