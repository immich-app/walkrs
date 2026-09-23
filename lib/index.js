import { walk as nativeWalk } from '../dist/index.js';

export async function* walk(options) {
  for await (const batch of nativeWalk(options)) {
    const parsed = JSON.parse(batch.toString());
    yield { ...parsed, files: parsed.files.map((entry) => typeof entry === 'string' ? entry : ({
      ...entry,
      modified: (() => {
        if (entry.modified == null) {
          throw new Error('Expected modified timestamp to be present when include_metadata is enabled.');
        }
        return new Date(parseInt(entry.modified) * 1000);
      })(),
    })) };
  }
}
