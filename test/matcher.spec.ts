import { Matcher, walkOrdered, type MatcherOptions, type WalkOrderedOptions } from '@immich/walkrs';
import { readdirSync } from 'node:fs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

const excludes = (pattern: string, file: string) =>
  !new Matcher({ exclusionPatterns: [pattern], includeHidden: true }).includes(file);

const literalNames = [
  'a@b',
  '#recycle',
  '$RECYCLE.BIN',
  '100%',
  'a^b',
  'rock & roll',
  'a=b',
  'my photos',
  'a,b',
  '(2020)',
  'a(b',
  '{x}',
  '{}',
  'a{b',
  'a}b',
  'a[b',
];

const random = (seed: number) => (max: number) => {
  seed = (Math.imul(seed, 1_664_525) + 1_013_904_223) >>> 0;
  return Math.floor((seed / 2 ** 32) * max);
};

const directoryNames = [
  'a',
  'B',
  'Raw',
  'raw',
  'Rawr',
  '@eaDir',
  '.hidden',
  '.thumbs',
  '1',
  '2',
  '4',
  '10',
  'Κύπρος',
  'κΥΠΡΟΣ',
  'my photos',
  '(2020)',
  '{x}',
  'a,b',
  '#recycle',
];

const fileNames = ['IMG_1', 'b', '._x', '.x', 'photo (1)', 'Κύπρος', 'a,b', 'Raw'];
const extensions = ['.jpg', '.JPG', '.png', '.tif', '.ARW', '.txt', ''];

const generateTree = (seed: number) => {
  const next = random(seed);
  const pick = <T>(values: T[]) => values[next(values.length)];
  const files = new Set(['lib/a.jpg', 'library/a.jpg', 'lib/.hidden/sub/b.jpg']);
  for (let i = 0; i < 150; i++) {
    const depth = 1 + next(4);
    const directories = Array.from({ length: depth }, () => pick(directoryNames));
    files.add([pick(['lib', 'library', 'other']), ...directories, pick(fileNames) + pick(extensions)].join('/'));
  }
  return [...files].filter((file) => ![...files].some((other) => other.startsWith(`${file}/`)));
};

const listFiles = (root: string) =>
  readdirSync(root, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => path.join(entry.parentPath, entry.name));

const walkFiles = async (options: WalkOrderedOptions) => {
  const files: string[] = [];
  for await (const batch of walkOrdered(options)) {
    files.push(...batch.files);
  }
  return files;
};

const optionSets = (root: string): MatcherOptions[] => [
  { paths: [root] },
  { paths: [`${root}/lib`, `${root}/library`], includeHidden: true },
  {
    paths: [`${root}/lib`, `${root}/lib/.hidden`, `${root}/library/a`, `${root}/other/.thumbs`],
    exclusionPatterns: ['**/Raw', '**/@eaDir/**', '**/*.tif', '**/._*'],
    extensions: ['.jpg', '.arw'],
  },
  {
    paths: [root, `${root}/other/Raw`, `${root}/lib/a/.hidden`],
    exclusionPatterns: ['**/{1..3}/**', '**/ΚΎΠΡΟΣ/**', `${root}/library/**`, '**/a/*.jpg', '**/{x}/**'],
  },
  {
    paths: [`${root}/other`, `${root}/lib/Raw`],
    includeHidden: true,
    exclusionPatterns: ['**/raw/**', '**/(2020)', '**/a,b/**', '**/*.png', '**/.thumbs/**'],
    extensions: ['jpg', '.tif', '.txt'],
  },
];

describe(Matcher.name, () => {
  describe('includes', () => {
    it.each([
      ['**/Raw/**', '/foo/Raw/bar.jpg', true],
      ['**/Raw/**', '/foo/bar.jpg', false],
      ['**/abc/*.tif', '/foo/abc/scan.tif', true],
      ['**/abc/*.tif', '/foo/abc/sub/scan.tif', false],
      ['**/*.tif', '/foo/bar.tif', true],
      ['**/*.jp?', '/foo/bar.jpg', true],
      ['**/@eaDir/**', '/foo/@eaDir/thumb.jpg', true],
      ['**/._*', '/foo/._resource', true],
      ['/absolute/path/**', '/absolute/path/photo.jpg', true],
      ['/absolute/path/**', '/absolute/path/2020/photo.jpg', true],
      ['/absolute/path/**', '/other/path/photo.jpg', false],
      ['/path/*.*', '/path/photo.jpg', true],
      ['/path/*.*', '/path/2020/photo.jpg', false],
      ['**/x/**', '/foo/x/y/z.jpg', true],
      ['**/raw/**', '/foo/RAW/bar.jpg', true],
      ['**/*.ARW', '/foo/bar.arw', true],
      ['**/İstanbul/**', '/foo/istanbul/a.jpg', false],
      ['**/ΚΎΠΡΟΣ/**', '/foo/Κύπρος/a.jpg', true],
      ['**/κύπροσ/**', '/foo/ΚΎΠΡΟΣ/a.jpg', true],
      [String.raw`**/a\y/**`, '/foo/a/b.jpg', false],
      [String.raw`**/a\k/**`, '/foo/ak/b.jpg', true],
      ['**/{a,b}/**', '/foo/b/c.jpg', true],
      ['**/{a,b}/**', '/foo/c/c.jpg', false],
      ['**/[abc].jpg', '/foo/b.jpg', true],
      ['**/[abc].jpg', '/foo/d.jpg', false],
      ['**/Raw/**', '/foo/Raw', true],
      ['**/Raw/**/**', '/foo/Raw', true],
    ])('excludes by %s for %s: %s', (pattern, file, excluded) => {
      expect(excludes(pattern, file)).toBe(excluded);
    });

    it.each(literalNames)('excludes a folder named %s literally', (name) => {
      const pattern = `**/${name}/**`;

      expect(excludes(pattern, `/foo/${name}/a.jpg`)).toBe(true);
      expect(excludes(pattern, '/foo/x/a.jpg')).toBe(false);
    });

    it.each([
      ['**/{1..3}/**', '/a/2/b.jpg', true],
      ['**/{1..3}/**', '/a/4/b.jpg', false],
      ['**/{8..11}/**', '/a/10/b.jpg', true],
      ['**/{8..11}/**', '/a/1/b.jpg', false],
      ['**/{3..1}/**', '/a/2/b.jpg', true],
      ['**/{a..c}/**', '/x/B/b.jpg', true],
      ['**/{a..c}/**', '/x/d/b.jpg', false],
      ['**/{01..03}/**', '/a/02/b.jpg', false],
      ['**/{01..03}/**', '/a/{01..03}/b.jpg', true],
      ['**/{x,{1..2}}/**', '/a/2/b.jpg', true],
    ])('expands the brace range %s for %s: %s', (pattern, file, excluded) => {
      expect(excludes(pattern, file)).toBe(excluded);
    });

    it.each(['**/{1..1001}/**', '**/{0..5000}/**'])('rejects the oversized brace range %s', (pattern) => {
      expect(() => new Matcher({ exclusionPatterns: [pattern] })).toThrow(
        expect.objectContaining({ code: 'InvalidArg' }),
      );
    });

    it.each([
      ['**/*(1).jpg', '/a/IMG(1).jpg', true],
      ['**/*(1).jpg', '/a/IMG1.jpg', false],
      ['**/*(1).jpg', '/a/11.jpg', false],
      ['**/?(a)/**', '/foo/x(a)/b.jpg', true],
      ['**/?(a)/**', '/foo/a/b.jpg', false],
      ['**/!(x)/**', '/foo/!(x)/b.jpg', true],
      ['**/!(x)/**', '/foo/a/b.jpg', false],
      ['**/@(a)/**', '/foo/@(a)/b.jpg', true],
      ['**/@(a)/**', '/foo/a/b.jpg', false],
      ['**/+(a|b)/**', '/foo/+(a|b)/c.jpg', true],
      ['**/+(a|b)/**', '/foo/a/c.jpg', false],
      ['!**/a/**', '/foo/b/c.jpg', false],
      ['!**/a/**', '/foo/a/c.jpg', false],
    ])('matches extglob and negation syntax %s literally for %s: %s', (pattern, file, excluded) => {
      expect(excludes(pattern, file)).toBe(excluded);
    });

    it('accepts a POSIX class pattern', () => {
      expect(() => new Matcher({ exclusionPatterns: ['**/[[:alpha:]]/**'] })).not.toThrow();
    });

    it('keeps a path under /lib apart from /library', () => {
      const sut = new Matcher({ paths: ['/lib'] });

      expect(sut.includes('/lib/a.jpg')).toBe(true);
      expect(sut.includes('/library/a.jpg')).toBe(false);
    });

    it.each(['/lib/', '/lib//', '/lib/sub/..', '/./lib'])('normalizes the path %s to /lib', (root) => {
      const sut = new Matcher({ paths: [root] });

      expect(sut.includes('/lib/a.jpg')).toBe(true);
      expect(sut.includes('/other/a.jpg')).toBe(false);
    });

    it.each(['', 'lib', 'lib/a.jpg'])('rejects the relative path %j', (root) => {
      expect(() => new Matcher({ paths: ['/lib', root] })).toThrow(
        expect.objectContaining({ code: 'InvalidArg', message: expect.stringContaining(`'${root}'`) }),
      );
    });

    it.each(['', 'lib/a.jpg', 'a.jpg', '/', '/lib/', '/lib//a.jpg', '/lib/./a.jpg', '/lib/x/../a.jpg', '/lib/a.jpg/'])(
      'returns false for the invalid input %j',
      (file) => {
        expect(new Matcher({ paths: ['/lib'] }).includes(file)).toBe(false);
      },
    );

    it('returns false without paths', () => {
      expect(new Matcher({ paths: [] }).includes('/lib/a.jpg')).toBe(false);
    });

    it('excludes a hidden folder or file below the path', () => {
      const sut = new Matcher({ paths: ['/lib'] });

      expect(sut.includes('/lib/.hidden/a.jpg')).toBe(false);
      expect(sut.includes('/lib/sub/.a.jpg')).toBe(false);
    });

    it('includes hidden folders and files with includeHidden', () => {
      const sut = new Matcher({ paths: ['/lib'], includeHidden: true });

      expect(sut.includes('/lib/.hidden/a.jpg')).toBe(true);
      expect(sut.includes('/lib/sub/.a.jpg')).toBe(true);
    });

    it('includes a path that is hidden or under a hidden folder', () => {
      const sut = new Matcher({ paths: ['/home/.config/.lib'] });

      expect(sut.includes('/home/.config/.lib/a.jpg')).toBe(true);
      expect(sut.includes('/home/.config/.lib/.b.jpg')).toBe(false);
    });

    it('includes a nested hidden path', () => {
      const sut = new Matcher({ paths: ['/lib', '/lib/.hidden'] });

      expect(sut.includes('/lib/.hidden/sub/b.jpg')).toBe(true);
      expect(sut.includes('/lib/.hidden/.x/b.jpg')).toBe(false);
      expect(sut.includes('/lib/.other/b.jpg')).toBe(false);
    });

    it('excludes files below a folder the exclusion matches', () => {
      const sut = new Matcher({ paths: ['/lib'], exclusionPatterns: ['**/Raw'] });

      expect(sut.includes('/lib/Raw/a.jpg')).toBe(false);
      expect(sut.includes('/lib/sub/raw/deep/a.jpg')).toBe(false);
      expect(sut.includes('/lib/Rawr/a.jpg')).toBe(true);
    });

    it('excludes files of a path the exclusion matches', () => {
      const sut = new Matcher({ paths: ['/lib/Raw', '/lib/sub'], exclusionPatterns: ['**/Raw'] });

      expect(sut.includes('/lib/Raw/a.jpg')).toBe(false);
      expect(sut.includes('/lib/sub/a.jpg')).toBe(true);
    });

    it('includes a path that is a file', () => {
      expect(new Matcher({ paths: ['/lib'] }).includes('/lib')).toBe(true);
      expect(new Matcher({ paths: ['/lib/a.jpg'] }).includes('/lib/a.jpg')).toBe(true);
      expect(new Matcher({ paths: ['/lib/.a.jpg'] }).includes('/lib/.a.jpg')).toBe(true);
      expect(new Matcher({ paths: ['/lib/a.jpg'], extensions: ['.png'] }).includes('/lib/a.jpg')).toBe(false);
    });

    it('filters by extension without case sensitivity', () => {
      const sut = new Matcher({ paths: ['/lib'], extensions: ['.jpg'] });

      expect(sut.includes('/lib/a.JPG')).toBe(true);
      expect(sut.includes('/lib/a.png')).toBe(false);
      expect(sut.includes('/lib/jpg')).toBe(false);
    });

    it('applies hidden and exclusion rules to every folder without paths', () => {
      const sut = new Matcher({ exclusionPatterns: ['**/Raw'] });

      expect(sut.includes('/home/user/a.jpg')).toBe(true);
      expect(sut.includes('/home/.config/a.jpg')).toBe(false);
      expect(sut.includes('/Raw/user/a.jpg')).toBe(false);
    });
  });

  describe('includesAll', () => {
    it('returns includes for every path', () => {
      const sut = new Matcher({ paths: ['/lib', '/lib/.hidden'], exclusionPatterns: ['**/Raw'], extensions: ['.jpg'] });
      const files = ['/lib/a.jpg', '/lib/a.png', '/lib/.x/a.jpg', '/lib/.hidden/a.jpg', '/lib/Raw/a.jpg', 'a.jpg', ''];

      expect(sut.includesAll(files)).toEqual([true, false, false, true, false, false, false]);
      expect(sut.includesAll(files)).toEqual(files.map((file) => sut.includes(file)));
    });
  });

  describe('on random trees', () => {
    let tempDir: string;

    beforeEach(async () => {
      tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'walkrs-matcher-test-'));
    });

    afterEach(async () => {
      await fs.rm(tempDir, { recursive: true, force: true });
    });

    it.each([1, 2, 3, 4, 5])('includes exactly the files walkOrdered returns for seed %s', async (seed) => {
      const files = generateTree(seed);
      for (const file of files) {
        await fs.mkdir(path.dirname(path.join(tempDir, file)), { recursive: true });
        await fs.writeFile(path.join(tempDir, file), '');
      }
      await fs.symlink(path.join(tempDir, 'lib/a.jpg'), path.join(tempDir, 'lib/link.jpg'));
      const onDisk = listFiles(tempDir);

      for (const options of optionSets(tempDir)) {
        const walked = new Set(await walkFiles({ ...options, paths: options.paths ?? [] }));
        const matched = new Set(
          new Matcher(options).includesAll(onDisk).flatMap((included, i) => (included ? [onDisk[i]] : [])),
        );

        expect({ options, onlyWalked: [...walked].filter((file) => !matched.has(file)) }).toEqual({
          options,
          onlyWalked: [],
        });
        expect({ options, onlyMatched: [...matched].filter((file) => !walked.has(file)) }).toEqual({
          options,
          onlyMatched: [],
        });
        expect(walked.size).toBeGreaterThan(0);
      }
    });
  });
});
