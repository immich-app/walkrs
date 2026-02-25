export { WalkOptions } from '../dist/index.js';

export type WalkError = {
  path?: string;
  message: string;
};

export type WalkBatch = {
  files: string[];
  errors: WalkError[];
};

export function walk(options: WalkOptions): AsyncGenerator<WalkBatch, void, unknown>;
