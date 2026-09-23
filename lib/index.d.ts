import type { WalkOptions } from '../dist/index.js';

export type { WalkOptions } from '../dist/index.js';

export type WalkError = {
  path?: string;
  message: string;
};

export interface WalkedFileEntry {
  path: string;
  modified: Date;
}

export type WalkBatch = {
  files: (string | WalkedFileEntry)[];
  errors: WalkError[];
};

export function walk(options: WalkOptions): AsyncGenerator<WalkBatch, void, unknown>;
