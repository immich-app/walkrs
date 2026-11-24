import { walk as nativeWalk, walkOrdered as nativeWalkOrdered } from '../dist/index.js';

export async function* walk(options) {
  for await (const batch of nativeWalk(options)) {
    yield JSON.parse(batch.toString());
  }
}

export async function* walkOrdered(options) {
  for await (const batch of nativeWalkOrdered(options)) {
    yield JSON.parse(batch.toString());
  }
}
