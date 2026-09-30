import type { WalkOptions } from '../dist/index.js';

export type { WalkOptions } from '../dist/index.js';

export type WalkError = {
  path?: string | null;
  message: string;
};

export type SidecarResult = string | null | { status: 'unknown' };

export type WalkBatch = {
  files: string[];
  size: number[] | null;
  modified: number[] | null;
  sidecars: SidecarResult[] | null;
  errors: WalkError[];
};

export function walk(options: WalkOptions): AsyncGenerator<WalkBatch, void, unknown>;
