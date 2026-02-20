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
      '/photos/image.jpg': true,
      '/photos/image.Jpg': true,
      '/photos/image.jpG': true,
      '/photos/image.JPG': true,
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
          // Filter for entries only (ignore errors) and extract paths
          const paths = batch.filter((item) => item.type === 'entry').map((item) => item.path);
          actual.push(...paths);
        }
        const expected = Object.entries(files)
          .filter((entry) => entry[1])
          .map(([file]) => path.join(tempDir, file.replace(/^\//, '')));

        expect([...actual].toSorted()).toEqual([...expected].toSorted());
      });
    });
  }

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
      const errors: Array<{ path?: string; message: string }> = [];

      for await (const batch of walk(options)) {
        for (const item of batch) {
          if (item.type === 'entry') {
            entries.push(item.path);
          } else if (item.type === 'error') {
            errors.push({ path: item.path, message: item.message });
          }
        }
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

      const entries: string[] = [];
      const errors: Array<{ path?: string; message: string }> = [];

      for await (const batch of walk(options)) {
        for (const item of batch) {
          if (item.type === 'entry') {
            entries.push(item.path);
          } else if (item.type === 'error') {
            errors.push({ path: item.path, message: item.message });
          }
        }
      }

      // Should have found all files (directory listing doesn't require file read permissions)
      expect(entries).toContain(path.join(tempDir, 'photos', 'accessible1.jpg'));
      expect(entries).toContain(path.join(tempDir, 'photos', 'accessible2.jpg'));

      // File is still listed even with 0o000 permissions (directory walk only needs directory read permission)
      expect(entries).toContain(path.join(tempDir, 'photos', 'restricted.jpg'));

      // No errors expected since we're only walking, not reading file contents
      expect(errors.length).toBe(0);

      // Restore permissions for cleanup
      await fs.chmod(path.join(tempDir, 'photos', 'restricted.jpg'), 0o644);
    });
  });
});
