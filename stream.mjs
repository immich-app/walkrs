let walkrs = await import('./dist/index.js');

const path = process.argv[2] || '/';

async function* walkStreamBatch(options) {
  const queue = [];
  let done = false;
  let resolve;
  let promise = new Promise((r) => (resolve = r));

  walkrs.walkStream(options, (batch) => {
    if (!batch) done = true;
    else queue.push(JSON.parse(batch));
    resolve();
  });

  while (true) {
    await promise;
    yield* queue.splice(0);
    if (done) return;
    promise = new Promise((r) => (resolve = r));
  }
}

const strings = [];
for await (const batch of walkStreamBatch({ paths: [path] })) {
  strings.push(...batch);
}

console.log(strings.length);
