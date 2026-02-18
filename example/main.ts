import { walk } from '@immich/walkrs';

const path = process.argv[2] || '/';

const files: string[] = [];
for await (const batch of walk({ paths: [path] })) {
  files.push(...batch);
}

console.log(files.length);
