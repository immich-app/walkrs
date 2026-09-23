import { walk, type WalkOptions } from '@immich/walkrs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

interface TestCase {
  test: string;
  options: WalkOptions;
  files: Record<string, boolean>;
}

const createTestFiles = async (basePath: string, files: string[]) => {
  await Promise.all(
    files.map(async (file) => {
      const fullPath = path.join(basePath, file.replace(/^\//, ''));
      await fs.mkdir(path.dirname(fullPath), { recursive: true });
      await fs.writeFile(fullPath, '');
    }),
  );
};

const mediaExtensions = [
  '.jpg',
  '.jpeg',
  '.heic',
  '.heif',
  '.png',
  '.gif',
  '.tif',
  '.tiff',
  '.webp',
  '.dng',
  '.nef',
  '.mp4',
  '.mov',
  '.webm',
];

const tests: TestCase[] = [
  {
    test: 'should return empty when crawling an empty path list',
    options: {
      paths: [],
    },
    files: {},
  },
  {
    test: 'should crawl a single path',
    options: {
      paths: ['/photos/'],
    },
    files: {
      '/photos/image.jpg': true,
    },
  },
  {
    test: 'should exclude by file extension',
    options: {
      paths: ['/photos/'],
      exclusionPatterns: ['**/*.tif'],
    },
    files: {
      '/photos/image.jpg': true,
      '/photos/image.tif': false,
    },
  },
  {
    test: 'should exclude by file extension without case sensitivity',
    options: {
      paths: ['/photos/'],
      exclusionPatterns: ['**/*.TIF'],
    },
    files: {
      '/photos/image.jpg': true,
      '/photos/image.tif': false,
      '/photos/image.tIf': false,
      '/photos/image.TIF': false,
    },
  },
  {
    test: 'should exclude by folder',
    options: {
      paths: ['/photos/'],
      exclusionPatterns: ['**/raw/**'],
    },
    files: {
      '/photos/image.jpg': true,
      '/photos/raw/image.jpg': false,
      '/photos/raw2/image.jpg': true,
      '/photos/folder/raw/image.jpg': false,
      '/photos/crawl/image.jpg': true,
    },
  },
  {
    test: 'should crawl multiple paths',
    options: {
      paths: ['/photos/', '/images/', '/albums/'],
    },
    files: {
      '/photos/image1.jpg': true,
      '/images/image2.jpg': true,
      '/albums/image3.jpg': true,
    },
  },
  {
    test: 'should crawl a single path without trailing slash',
    options: {
      paths: ['/photos'],
    },
    files: {
      '/photos/image.jpg': true,
    },
  },
  {
    test: 'should crawl a single path',
    options: {
      paths: ['/photos/'],
    },
    files: {
      '/photos/image.jpg': true,
      '/photos/subfolder/image1.jpg': true,
      '/photos/subfolder/image2.jpg': true,
      '/image1.jpg': false,
    },
  },
  {
    test: 'should filter file extensions',
    options: {
      paths: ['/photos/'],
      extensions: ['.jpg'],
    },
    files: {
      '/photos/image.jpg': true,
      '/photos/image.txt': false,
      '/photos/1': false,
    },
  },
  {
    test: 'should include photo and video extensions',
    options: {
      paths: ['/photos/', '/videos/'],
      extensions: mediaExtensions,
    },
    files: {
      '/photos/image.jpg': true,
      '/photos/image.jpeg': true,
      '/photos/image.heic': true,
      '/photos/image.heif': true,
      '/photos/image.png': true,
      '/photos/image.gif': true,
      '/photos/image.tif': true,
      '/photos/image.tiff': true,
      '/photos/image.webp': true,
      '/photos/image.dng': true,
      '/photos/image.nef': true,
      '/videos/video.mp4': true,
      '/videos/video.mov': true,
      '/videos/video.webm': true,
    },
  },
  {
    test: 'should check file extensions without case sensitivity',
    options: {
      paths: ['/photos/'],
      extensions: ['.jpg', '.jpeg', '.tiff', '.tif', '.dng', '.nef'],
    },
    files: {
      '/photos/image1.jpg': true,
      '/photos/image2.Jpg': true,
      '/photos/image3.jpG': true,
      '/photos/image4.JPG': true,
      '/photos/image.jpEg': true,
      '/photos/image.TIFF': true,
      '/photos/image.tif': true,
      '/photos/image.dng': true,
      '/photos/image.NEF': true,
    },
  },
  {
    test: 'should normalize the path',
    options: {
      paths: ['/photos/1/../2'],
    },
    files: {
      '/photos/1/image.jpg': false,
      '/photos/2/image.jpg': true,
    },
  },
  {
    test: 'should support special characters in paths',
    options: {
      paths: ['/photos (new)'],
    },
    files: {
      '/photos (new)/1.jpg': true,
    },
  },
];

describe('walk', () => {
  for (const { test, options, files } of tests) {
    describe(test, () => {
      const fileList = Object.keys(files);
      let tempDir: string;

      beforeEach(async () => {
        tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'walkrs-test-'));
        await createTestFiles(tempDir, fileList);
      });

      afterEach(async () => {
        await fs.rm(tempDir, { recursive: true, force: true });
      });

      it('returns expected files', async () => {
        const adjustedOptions: WalkOptions = {
          ...options,
          paths: options.paths.map((p: string) => path.join(tempDir, p.replace(/^\//, ''))),
        };

        const actual: string[] = [];
        for await (const batch of walk(adjustedOptions)) {
          expect(batch.size).toBeNull();
          expect(batch.modified).toBeNull();
          actual.push(...batch.files);
        }
        const expected = Object.entries(files)
          .filter((entry) => entry[1])
          .map(([file]) => path.join(tempDir, file.replace(/^\//, '')));

        expect([...actual].toSorted()).toEqual([...expected].toSorted());
      });
    });
  }

  describe('metadata', () => {
    let tempDir: string;

    beforeEach(async () => {
      tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'walkrs-metadata-'));
    });

    afterEach(async () => {
      await fs.chmod(path.join(tempDir, 'restricted'), 0o755).catch(() => {});
      await fs.rm(tempDir, { recursive: true, force: true });
    });

    it.each([1, 2, 0])('aligns size and millisecond timestamps with paths using %i threads', async (threads) => {
      const expected = new Map<string, { size: number; modified: number }>();
      for (const [name, content, time] of [
        ['empty.jpg', '', new Date(1_700_000_000_123)],
        ['quoted"雪.jpg', 'hello world', new Date(1_700_000_000_789)],
        ['ignored.txt', 'ignored', new Date(1_700_000_000_456)],
      ] as const) {
        const filename = path.join(tempDir, name);
        await fs.writeFile(filename, content);
        await fs.utimes(filename, time, time);
        const stat = await fs.stat(filename, { bigint: true });
        if (name.endsWith('.jpg')) {
          expected.set(filename, { size: Number(stat.size), modified: Number(stat.mtimeNs / 1_000_000n) });
        }
      }

      const seen = new Set<string>();
      for await (const batch of walk({ paths: [tempDir], includeMetadata: true, extensions: ['jpg'], threads })) {
        expect(batch.errors).toEqual([]);
        expect(batch.size).toHaveLength(batch.files.length);
        expect(batch.modified).toHaveLength(batch.files.length);
        for (const [index, filename] of batch.files.entries()) {
          expect({ size: batch.size![index], modified: batch.modified![index] }).toEqual(expected.get(filename));
          expect(Number.isSafeInteger(batch.size![index])).toBe(true);
          expect(Number.isSafeInteger(batch.modified![index])).toBe(true);
          seen.add(filename);
        }
      }
      expect(seen).toEqual(new Set(expected.keys()));
    });

    it('supports modification times before the Unix epoch', async () => {
      const filename = path.join(tempDir, 'old.jpg');
      await fs.writeFile(filename, 'old');
      await fs.utimes(filename, new Date(-1234), new Date(-1234));
      const stat = await fs.stat(filename, { bigint: true });
      const batches = [];
      for await (const batch of walk({ paths: [tempDir], includeMetadata: true })) {
        batches.push(batch);
      }
      expect(batches).toEqual([
        {
          files: [filename],
          size: [3],
          modified: [Number(stat.mtimeNs / 1_000_000n)],
          errors: [],
        },
      ]);
    });

    it('keeps metadata aligned across full and partial batches', async () => {
      const names = Array.from({ length: 4101 }, (_, i) => `${i}.jpg`);
      await createTestFiles(tempDir, names);
      const seen = new Set<string>();
      const lengths = [];
      for await (const batch of walk({ paths: [tempDir], threads: 1, includeMetadata: true })) {
        expect(batch.errors).toEqual([]);
        expect(batch.size).toEqual(Array.from({ length: batch.files.length }, () => 0));
        expect(batch.modified).toHaveLength(batch.files.length);
        expect(batch.modified!.every((value) => Number.isSafeInteger(value))).toBe(true);
        lengths.push(batch.files.length);
        for (const filename of batch.files) {
          expect(seen.has(filename)).toBe(false);
          seen.add(filename);
        }
      }
      expect(lengths).toEqual([4096, 5]);
      expect(seen).toEqual(new Set(names.map((name) => path.join(tempDir, name))));
    });

    it('reports metadata failures without dropping successful files or misaligning columns', async () => {
      const restricted = path.join(tempDir, 'restricted');
      const inaccessible = path.join(restricted, 'image.jpg');
      const accessible = path.join(tempDir, 'image.jpg');
      await createTestFiles(tempDir, ['restricted/image.jpg', 'image.jpg']);
      // Read permission allows enumeration; lack of search permission prevents stat on its children.
      await fs.chmod(restricted, 0o444);

      const listed = [];
      for await (const batch of walk({ paths: [tempDir], includeMetadata: false })) {
        listed.push(...batch.files);
        expect(batch.size).toBeNull();
        expect(batch.modified).toBeNull();
      }
      expect(listed).toContain(inaccessible);

      const found = [];
      const errors = [];
      for await (const batch of walk({ paths: [tempDir], includeMetadata: true })) {
        expect(batch.size).toHaveLength(batch.files.length);
        expect(batch.modified).toHaveLength(batch.files.length);
        found.push(...batch.files);
        errors.push(...batch.errors);
      }
      expect(found).toEqual([accessible]);
      expect(errors).toEqual(expect.arrayContaining([expect.objectContaining({ path: inaccessible })]));
    });

    it('uses empty arrays for an error-only metadata batch', async () => {
      const batches = [];
      for await (const batch of walk({ paths: [path.join(tempDir, 'missing')], includeMetadata: true })) {
        batches.push(batch);
      }
      expect(batches).toHaveLength(1);
      expect(batches[0]).toMatchObject({ files: [], size: [], modified: [] });
      expect(batches[0].errors).toHaveLength(1);
    });

    it('returns no batches for an empty metadata walk', async () => {
      const batches = [];
      for await (const batch of walk({ paths: [], includeMetadata: true })) {
        batches.push(batch);
      }
      expect(batches).toEqual([]);
    });
  });

  describe('error handling', () => {
    let tempDir: string;

    beforeEach(async () => {
      tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'walkrs-test-'));
    });

    afterEach(async () => {
      // Restore permissions before cleanup
      try {
        await fs.chmod(path.join(tempDir, 'restricted'), 0o755);
      } catch {
        // Ignore if directory doesn't exist
      }
      await fs.rm(tempDir, { recursive: true, force: true });
    });

    it('reports access denied errors for restricted directory', async () => {
      // Create a directory structure with a restricted directory
      await fs.mkdir(path.join(tempDir, 'accessible'), { recursive: true });
      await fs.mkdir(path.join(tempDir, 'restricted'), { recursive: true });
      await fs.writeFile(path.join(tempDir, 'accessible', 'file1.jpg'), '');
      await fs.writeFile(path.join(tempDir, 'restricted', 'file2.jpg'), '');

      // Remove all permissions from the restricted directory
      await fs.chmod(path.join(tempDir, 'restricted'), 0o000);

      const options: WalkOptions = {
        paths: [tempDir],
        extensions: ['.jpg'],
      };

      const entries: string[] = [];
      const errors: Array<{ path?: string | null; message: string }> = [];

      for await (const batch of walk(options)) {
        entries.push(...batch.files);
        errors.push(...batch.errors);
      }

      // Should have found the accessible file
      expect(entries).toContain(path.join(tempDir, 'accessible', 'file1.jpg'));

      // Should have reported at least one error for the restricted directory
      expect(errors.length).toBeGreaterThan(0);
      expect(errors.some((error) => error.message.toLowerCase().includes('permission denied'))).toBe(true);
    });

    it('can still enumerate files with restricted permissions', async () => {
      // Create a directory with multiple files, one of which is restricted
      await fs.mkdir(path.join(tempDir, 'photos'), { recursive: true });
      await fs.writeFile(path.join(tempDir, 'photos', 'accessible1.jpg'), '');
      await fs.writeFile(path.join(tempDir, 'photos', 'restricted.jpg'), '');
      await fs.writeFile(path.join(tempDir, 'photos', 'accessible2.jpg'), '');

      // Remove all permissions from a single file
      await fs.chmod(path.join(tempDir, 'photos', 'restricted.jpg'), 0o000);

      const options: WalkOptions = {
        paths: [tempDir],
        extensions: ['.jpg'],
      };

      const files: string[] = [];
      const errors: Array<{ path?: string | null; message: string }> = [];

      for await (const batch of walk(options)) {
        files.push(...batch.files);
        errors.push(...batch.errors);
      }

      expect(files).toContain(path.join(tempDir, 'photos', 'accessible1.jpg'));
      expect(files).toContain(path.join(tempDir, 'photos', 'accessible2.jpg'));

      // File is still listed even with 0o000 permissions (directory walk only needs directory read permission)
      expect(files).toContain(path.join(tempDir, 'photos', 'restricted.jpg'));

      expect(errors.length).toBe(0);

      await fs.chmod(path.join(tempDir, 'photos', 'restricted.jpg'), 0o644);
    });
  });
});
