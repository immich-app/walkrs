let walkrs = await import('./dist/index.js');

const path = process.argv[2] || '/';
console.log((await walkrs.walk({ paths: [path] })).length);
