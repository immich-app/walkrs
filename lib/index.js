import { walk as nativeWalk } from '../dist/index.js';

export async function* walk(options) {
  for await (const batch of nativeWalk(options)) {
    yield JSON.parse(batch.toString());
  }
}
