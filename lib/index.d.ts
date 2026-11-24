import type { WalkOptions, WalkOrderedOptions } from '../dist/index.js';

export { Matcher } from '../dist/index.js';
export type { MatcherOptions, WalkOptions, WalkOrderedOptions } from '../dist/index.js';

export type WalkError = {
  path: string | null;
  message: string;
};

export type WalkBatch = {
  files: string[];
  errors: WalkError[];
};

export type WalkOrderedBatch = {
  files: string[];
  modified: number[];
  errors: WalkError[];
};

export function walk(options: WalkOptions): AsyncGenerator<WalkBatch, void, unknown>;

export function walkOrdered(options: WalkOrderedOptions): AsyncGenerator<WalkOrderedBatch, void, unknown>;
