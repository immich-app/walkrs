import { walk, walkOrdered, type WalkError, type WalkOrderedOptions } from '@immich/walkrs';
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, expectTypeOf, it } from 'vitest';

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

const touch = (times: Array<[time: string, file: string]>) =>
  execFileSync('sh', ['-c', 'while [ $# -gt 0 ]; do touch -d "@$1" "$2"; shift 2; done', 'sh', ...times.flat()]);

const randomTimes = (count: number) => {
  let seed = 42;
  const random = (max: number) => {
    seed = (Math.imul(seed, 1_664_525) + 1_013_904_223) >>> 0;
    return Math.floor((seed / 2 ** 32) * max);
  };
  return Array.from({ length: count }, () => {
    const seconds = random(6_000_000_000) - 2_000_000_000;
    const nanos = random(2) === 0 ? random(1_000_000_000) : random(1000) * 1_000_000 + 499_800 + random(400);
    return `${seconds}.${String(nanos).padStart(9, '0')}`;
  });
};

const withInvalidUtf8 = (prefix: string, suffix = '') => Buffer.from(`${prefix}\u00FF${suffix}`, 'latin1');

const isAscending = (files: string[]) =>
  files.every((file, i) => i === 0 || Buffer.compare(Buffer.from(files[i - 1]), Buffer.from(file)) < 0);

describe('walkOrdered', () => {
  let tempDir: string;

  const options = () => ({
    paths: [tempDir],
    exclusionPatterns: ['**/@eaDir/**', '**/*.tif'],
    extensions: ['.jpg', '.png'],
  });

  const expectOrder = async (files: string[]) => {
    await writeFiles(tempDir, files.toReversed());

    const actual = await collect({ paths: [tempDir] });

    expect(actual.files).toEqual(files.map((file) => path.join(tempDir, file)));
  };

  const expectStatTimes = async (times: string[]) => {
    const names = times.map((_, i) => `${i}.jpg`);
    await writeFiles(tempDir, names);
    touch(times.map((time, i) => [time, path.join(tempDir, names[i])]));

    const { files, modified } = await collect({ paths: [tempDir] });

    expect(files).toHaveLength(times.length);
    expect(modified).toEqual(files.map((file) => statSync(file).mtime.getTime()));
  };

  const walkPaths = async (paths: string[], options: Omit<WalkOrderedOptions, 'paths'> = {}) => {
    const { files, errors } = await collect({ ...options, paths: paths.map((path) => tempDir + path) });
    return { files: files.map((file) => file.slice(tempDir.length)), errors };
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
      for await (const batch of walk(options())) {
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

  describe('with several paths', () => {
    it('returns disjoint paths in byte order', async () => {
      await writeFiles(tempDir, ['a/x.jpg', 'a-b/x.jpg', 'b/x.jpg']);

      const { files } = await walkPaths(['/b', '/a', '/a-b']);

      expect(files).toEqual(['/a-b/x.jpg', '/a/x.jpg', '/b/x.jpg']);
    });

    it('returns a file of nested paths once', async () => {
      await writeFiles(tempDir, ['lib/a.jpg', 'lib/sub/b.jpg', 'lib/sub/deep/c.jpg', 'lib/z.jpg']);

      const { files } = await walkPaths(['/lib/sub/deep', '/lib', '/lib/sub']);

      expect(files).toEqual(['/lib/a.jpg', '/lib/sub/b.jpg', '/lib/sub/deep/c.jpg', '/lib/z.jpg']);
    });

    it.each(['/lib/.hidden', '/lib/.hidden/sub'])('returns a nested hidden path %s', async (path) => {
      await writeFiles(tempDir, ['lib/.hidden/sub/b.jpg', 'lib/a.jpg', 'lib/z.jpg']);

      const { files } = await walkPaths([path, '/lib'], { includeHidden: false });

      expect(files).toEqual(['/lib/.hidden/sub/b.jpg', '/lib/a.jpg', '/lib/z.jpg']);
    });

    it.each(['**/Raw/**', '**/Raw'])('skips a nested path the exclusion %s matches', async (pattern) => {
      await writeFiles(tempDir, ['lib/Raw/b.jpg', 'lib/a.jpg']);

      const { files } = await walkPaths(['/lib/Raw', '/lib'], { exclusionPatterns: [pattern] });

      expect(files).toEqual(['/lib/a.jpg']);
    });

    it.each(['/lib/', '/lib//', '/lib/sub/..', '/lib/.', '/./lib'])('walks %s as /lib', async (path) => {
      await writeFiles(tempDir, ['lib/a.jpg', 'lib/sub/b.jpg']);

      const { files } = await walkPaths([path, '/lib']);

      expect(files).toEqual(['/lib/a.jpg', '/lib/sub/b.jpg']);
    });

    it('walks a path that is a symlink to a directory', async () => {
      await writeFiles(tempDir, ['lib/a.jpg', 'target/b.jpg']);
      await fs.symlink(path.join(tempDir, 'target'), path.join(tempDir, 'lib/link'));

      const { files } = await walkPaths(['/lib', '/lib/link']);

      expect(files).toEqual(['/lib/a.jpg', '/lib/link/b.jpg']);
    });

    it('returns nothing without paths', async () => {
      await expect(collect({ paths: [] })).resolves.toEqual({ files: [], modified: [], errors: [], batches: 0 });
    });

    it.skipIf(process.getuid?.() === 0)('reports an unreadable directory of nested paths once', async () => {
      await writeFiles(tempDir, ['a.jpg', 'restricted/b.jpg', 'z.jpg']);
      await fs.chmod(path.join(tempDir, 'restricted'), 0o000);

      const { files, errors } = await walkPaths(['', '/restricted']);

      expect(files).toEqual(['/a.jpg', '/z.jpg']);
      expect(errors).toEqual([{ path: path.join(tempDir, 'restricted'), message: expect.any(String) }]);
    });
  });

  describe.skipIf(process.platform !== 'linux')('modified', () => {
    it.each([
      '1700000000.000499900',
      '1700000000.000500000',
      '1700000000.000500100',
      '1700000000.123499900',
      '-1000000000.000499900',
      '-1.000500000',
      '32503680000.000499900',
    ])('returns the fs.stat time for %s', (time) => expectStatTimes([time]));

    it('returns the fs.stat time for random nanosecond times', () => expectStatTimes(randomTimes(200)));
  });

  describe('threads', () => {
    it.each([-1, 1e9, 0, 1.9, Number.NaN])('walks with %s threads', async (threads) => {
      await writeFiles(tempDir, ['a.jpg', 'b/c.jpg', 'b/d/e.jpg']);

      const { files, errors } = await walkPaths([''], { threads });

      expect(files).toEqual(['/a.jpg', '/b/c.jpg', '/b/d/e.jpg']);
      expect(errors).toEqual([]);
    });
  });

  describe('paths', () => {
    it.each(['', '.', 'lib', 'lib/a.jpg'])('rejects the relative path %j', async (relative) => {
      await expect(collect({ paths: [tempDir, relative] })).rejects.toMatchObject({
        code: 'InvalidArg',
        message: expect.stringContaining(`'${relative}'`),
      });
    });
  });

  it.each([1, 4])('returns a wide directory in byte order with %s threads', async (threads) => {
    const tricky = [
      'a',
      'a b/x.jpg',
      'a-b/x.jpg',
      'a.jpg',
      'a0/x.jpg',
      'ab',
      'b.jpg',
      'b/x.jpg',
      '～/x',
      '～.jpg',
      '😀.jpg',
    ];
    const files = [...Array.from({ length: 20_000 }, (_, i) => `${i}.jpg`), ...tricky];
    await writeFiles(tempDir, files);
    const expected = files
      .map((file) => path.join(tempDir, file))
      .toSorted((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));

    const actual = await collect({ paths: [tempDir], threads });

    expect(actual.files).toEqual(expected);
  });

  it.skipIf(!existsSync('/dev/shm'))('reports names that are not UTF-8', async () => {
    const root = await fs.mkdtemp('/dev/shm/walkrs-ordered-test-');
    try {
      await fs.mkdir(withInvalidUtf8(`${root}/dir`));
      await Promise.all(
        [
          withInvalidUtf8(`${root}/bad`, '.jpg'),
          withInvalidUtf8(`${root}/dir`, '/ok.jpg'),
          path.join(root, 'good.jpg'),
        ].map((file) => fs.writeFile(file, '')),
      );

      const { files, errors } = await collect({ paths: [root] });

      expect(files).toEqual([path.join(root, 'good.jpg')]);
      expect(errors.toSorted((a, b) => (a.path ?? '').localeCompare(b.path ?? ''))).toEqual([
        { path: `${root}/bad\uFFFD.jpg`, message: 'Path is not valid UTF-8' },
        { path: `${root}/dir\uFFFD`, message: 'Path is not valid UTF-8' },
      ]);
    } finally {
      execFileSync('rm', ['-rf', root]);
    }
  });

  it('types a missing error path as null', () => {
    expectTypeOf<WalkError['path']>().toEqualTypeOf<string | null>();
  });

  it.skipIf(process.platform !== 'linux')('stops a walk without matches when returned during next', async () => {
    const directories = Array.from({ length: 50 }, (_, i) => Array.from({ length: 100 }, (_, j) => `${i}/${j}`)).flat();
    await Promise.all(directories.map((directory) => fs.mkdir(path.join(tempDir, directory), { recursive: true })));
    await fs.writeFile(path.join(tempDir, 'z.jpg'), '');
    const before = threadCount();
    const batches = walkOrdered({ paths: [tempDir], extensions: ['jpg'], threads: 4 });

    const pending = batches.next();
    await batches.return();

    await expect(pending).resolves.toEqual({ done: true, value: undefined });
    await expect.poll(threadCount).toBeLessThanOrEqual(before);
  });

  it.skipIf(process.platform !== 'linux')('stops its threads when the consumer breaks early', async () => {
    const files = Array.from({ length: 100 }, (_, i) => Array.from({ length: 800 }, (_, j) => `${i}/${j}.jpg`)).flat();
    await writeFiles(tempDir, files);
    const before = threadCount();

    for await (const batch of walkOrdered({ paths: [tempDir], threads: 4 })) {
      expect(batch.files.length).toBeGreaterThan(0);
      break;
    }

    await expect.poll(threadCount).toBeLessThanOrEqual(before);
  });

  it.skipIf(process.getuid?.() === 0)('reports an unreadable directory and continues', async () => {
    await writeFiles(tempDir, ['a.jpg', 'restricted/b.jpg', 'z.jpg']);
    await fs.chmod(path.join(tempDir, 'restricted'), 0o000);

    const { files, errors } = await collect({ paths: [tempDir] });

    expect(files).toEqual([path.join(tempDir, 'a.jpg'), path.join(tempDir, 'z.jpg')]);
    expect(errors).toEqual([
      { path: path.join(tempDir, 'restricted'), message: expect.stringMatching(/permission denied/i) },
    ]);
  });
});
