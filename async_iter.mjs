let walkrs = await import('./dist/index.js');

const path = process.argv[2] || '/';

const strings = [];
const iter = walkrs.walkAsyncIter({ paths: [path] });
let batch;
while ((batch = await iter.next()) !== null) {
  strings.push(...JSON.parse(batch));
}

console.log(strings.length);
