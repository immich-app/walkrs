import { walk, walkOrdered, type WalkError, type WalkOrderedOptions } from '@immich/walkrs';
import { readdirSync } from 'node:fs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

const collect = async (options: WalkOrderedOptions) => {
  const files: string[] = [];
  const modified: number[] = [];
  const errors: WalkError[] = [];
  let batches = 0;
  for await (const batch of walkOrdered(options)) {
    batches++;
    files.push(...batch.files);
    modified.push(...batch.modified);
    errors.push(...batch.errors);
  }
  return { files, modified, errors, batches };
};

const writeFiles = async (root: string, files: string[]) => {
  for (const directory of new Set(files.map((file) => path.dirname(path.join(root, file))))) {
    await fs.mkdir(directory, { recursive: true });
  }
  await Promise.all(files.map((file) => fs.writeFile(path.join(root, file), '')));
};

const generateTree = () => {
  const extensions = ['jpg', 'JPG', 'png', 'txt', 'tif'];
  const files: string[] = [];
  for (let i = 0; i < 30; i++) {
    for (let j = 0; j < 10; j++) {
      for (let k = 0; k < 64; k++) {
        files.push(`album ${i}/day-${j}/IMG_${k}.${extensions[k % extensions.length]}`);
      }
      files.push(`album ${i}/day-${j}.jpg`, `album ${i}/.hidden-${j}.jpg`, `album ${i}/.thumbs/${j}.jpg`);
    }
    files.push(`album ${i}/@eaDir/thumb.jpg`, `album ${i}/@eaDir/nested/thumb.jpg`);
  }
  return files;
};

const threadCount = () => readdirSync('/proc/self/task').length;

const isAscending = (files: string[]) =>
  files.every((file, i) => i === 0 || Buffer.compare(Buffer.from(files[i - 1]), Buffer.from(file)) < 0);

describe('walkOrdered', () => {
  let tempDir: string;

  const options = () => ({
    path: tempDir,
    exclusionPatterns: ['**/@eaDir/**', '**/*.tif'],
    extensions: ['.jpg', '.png'],
  });

  const expectOrder = async (files: string[]) => {
    await writeFiles(tempDir, files.toReversed());

    const actual = await collect({ path: tempDir });

    expect(actual.files).toEqual(files.map((file) => path.join(tempDir, file)));
  };

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'walkrs-ordered-test-'));
  });

  afterEach(async () => {
    await fs.chmod(path.join(tempDir, 'restricted'), 0o755).catch(() => {});
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe('on a large tree', () => {
    beforeEach(async () => {
      await writeFiles(tempDir, generateTree());
      await fs.utimes(path.join(tempDir, 'album 0/day-0.jpg'), 1, 0.0006);
      await fs.utimes(path.join(tempDir, 'album 1/day-0.jpg'), 1, '-1.0000005');
    });

    it('returns the same files as walk', async () => {
      const expected: string[] = [];
      for await (const batch of walk({ ...options(), paths: [tempDir] })) {
        expected.push(...batch.files);
      }

      const { files, errors, batches } = await collect(options());

      expect(files.length).toBeGreaterThan(10_000);
      expect(batches).toBeGreaterThan(2);
      expect(errors).toEqual([]);
      expect(new Set(files).size).toBe(files.length);
      expect(files.toSorted()).toEqual(expected.toSorted());
    });

    it('returns files in strictly ascending byte order', async () => {
      const { files } = await collect(options());

      expect(isAscending(files)).toBe(true);
    });

    it('returns modified times equal to fs.stat', async () => {
      const { files, modified } = await collect(options());

      const stats = await Promise.all(files.map((file) => fs.stat(file)));
      const expected = stats.map((stat) => stat.mtime.valueOf());
      expect(modified).toEqual(expected);
      expect(modified[files.indexOf(path.join(tempDir, 'album 0/day-0.jpg'))]).toBe(1);
      expect(modified[files.indexOf(path.join(tempDir, 'album 1/day-0.jpg'))]).toBe(-1000);
    });

    it('returns the same files with a single thread', async () => {
      const { files: expected } = await collect(options());

      const { files } = await collect({ ...options(), threads: 1 });

      expect(files).toEqual(expected);
    });
  });

  describe('ordering', () => {
    it('returns a file before a directory with the same stem', () => expectOrder(['a.jpg', 'a/b.jpg']));

    it('returns names with space and dash before the slash', () =>
      expectOrder(['a b/c.jpg', 'a-b/c.jpg', 'a.jpg', 'a/c.jpg', 'ab.jpg']));

    it('returns names in UTF-8 byte order', () => expectOrder(['～.jpg', '～/a.jpg', '😀.jpg']));
  });

  it.skipIf(process.platform !== 'linux')('stops its threads when the consumer breaks early', async () => {
    const files = Array.from({ length: 100 }, (_, i) => Array.from({ length: 800 }, (_, j) => `${i}/${j}.jpg`)).flat();
    await writeFiles(tempDir, files);
    const before = threadCount();

    for await (const batch of walkOrdered({ path: tempDir, threads: 4 })) {
      expect(batch.files.length).toBeGreaterThan(0);
      break;
    }

    await expect.poll(threadCount).toBeLessThanOrEqual(before);
  });

  it.skipIf(process.getuid?.() === 0)('reports an unreadable directory and continues', async () => {
    await writeFiles(tempDir, ['a.jpg', 'restricted/b.jpg', 'z.jpg']);
    await fs.chmod(path.join(tempDir, 'restricted'), 0o000);

    const { files, errors } = await collect({ path: tempDir });

    expect(files).toEqual([path.join(tempDir, 'a.jpg'), path.join(tempDir, 'z.jpg')]);
    expect(errors).toEqual([
      { path: path.join(tempDir, 'restricted'), message: expect.stringMatching(/permission denied/i) },
    ]);
  });
});
