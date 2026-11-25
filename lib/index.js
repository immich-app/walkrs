import { walk as nativeWalk, walkOrdered as nativeWalkOrdered } from '../dist/index.js';

export { Matcher } from '../dist/index.js';

export async function* walk(options) {
  for await (const batch of nativeWalk(options)) {
    yield JSON.parse(batch.toString());
  }
}

export function walkOrdered(options) {
  const batches = nativeWalkOrdered(options)[Symbol.asyncIterator]();
  return {
    async next() {
      const { done, value } = await batches.next();
      return done ? { done, value: undefined } : { done, value: JSON.parse(value.toString()) };
    },
    return: (value) => batches.return(value),
    throw: (error) => batches.throw(error),
    [Symbol.asyncIterator]() {
      return this;
    },
  };
}
