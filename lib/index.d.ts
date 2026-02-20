export { WalkOptions } from '../dist/index.js';

export type WalkEntry = {
  type: 'entry';
  path: string;
};

export type WalkError = {
  type: 'error';
  path?: string;
  message: string;
};

export type WalkItem = WalkEntry | WalkError;

export function walk(options: WalkOptions): AsyncGenerator<WalkItem[], void, unknown>;
