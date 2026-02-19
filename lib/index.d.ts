export { WalkOptions } from '../dist/index.js';

export interface WalkedFileEntry {
	path: string;
	modified: Date;
}

export function walk(options: WalkOptions): AsyncGenerator<WalkedFileEntry[], void, unknown>;
