import { walk, type SidecarResult, type WalkOptions } from '@immich/walkrs';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

describe('sidecar discovery', () => {
  let directory: string;

  beforeEach(async () => {
    directory = await fs.mkdtemp(path.join(os.tmpdir(), 'walkrs-sidecars-'));
  });

  afterEach(async () => {
    await fs.rm(directory, { recursive: true, force: true });
  });

  async function create(names: string[]) {
    for (const name of names) {
      const filename = path.join(directory, name);
      await fs.mkdir(path.dirname(filename), { recursive: true });
      await fs.writeFile(filename, '');
    }
  }

  async function scan(options: Partial<WalkOptions> = {}) {
    const found = new Map<string, SidecarResult>();
    for await (const batch of walk({
      paths: [directory],
      extensions: ['jpg', 'raw'],
      includeSidecars: true,
      ...options,
    })) {
      expect(batch.errors).toEqual([]);
      expect(batch.sidecars).toHaveLength(batch.files.length);
      if (options.includeMetadata) {
        expect(batch.size).toHaveLength(batch.files.length);
        expect(batch.modified).toHaveLength(batch.files.length);
      } else {
        expect(batch.size).toBeNull();
        expect(batch.modified).toBeNull();
      }
      for (const [index, filename] of batch.files.entries()) {
        expect(found.has(filename)).toBe(false);
        found.set(filename, batch.sidecars![index]);
      }
    }
    return found;
  }

  it.each([1, 2, 0])('preserves candidate precedence and shared stems with %i threads', async (threads) => {
    await create(['file.xmp', 'file.jpg.xmp', 'file.raw', 'file.jpg', 'missing.jpg', 'orphan.xmp']);
    expect(await scan({ threads, includeMetadata: true })).toEqual(
      new Map([
        [path.join(directory, 'file.jpg'), path.join(directory, 'file.jpg.xmp')],
        [path.join(directory, 'file.raw'), path.join(directory, 'file.xmp')],
        [path.join(directory, 'missing.jpg'), null],
      ]),
    );
  });

  it('falls back from unreadable and broken preferred sidecars', async () => {
    await create(['unreadable.jpg', 'unreadable.jpg.xmp', 'unreadable.xmp', 'broken.jpg', 'broken.xmp', 'absent.jpg']);
    await fs.chmod(path.join(directory, 'unreadable.jpg.xmp'), 0o000);
    await fs.symlink(path.join(directory, 'missing.xmp'), path.join(directory, 'broken.jpg.xmp'));
    expect(await scan()).toEqual(
      new Map([
        [path.join(directory, 'unreadable.jpg'), path.join(directory, 'unreadable.xmp')],
        [path.join(directory, 'broken.jpg'), path.join(directory, 'broken.xmp')],
        [path.join(directory, 'absent.jpg'), null],
      ]),
    );
  });

  it('follows sidecar symlinks and retains directory candidates like access(R_OK)', async () => {
    await create(['link.jpg', 'target/metadata.xmp', 'folder.jpg']);
    await fs.symlink(path.join(directory, 'target/metadata.xmp'), path.join(directory, 'link.jpg.xmp'));
    await fs.mkdir(path.join(directory, 'folder.jpg.xmp'));
    expect(await scan()).toEqual(
      new Map([
        [path.join(directory, 'link.jpg'), path.join(directory, 'link.jpg.xmp')],
        [path.join(directory, 'folder.jpg'), path.join(directory, 'folder.jpg.xmp')],
      ]),
    );
  });

  it('resolves sidecars beside symlinked media when following links is enabled', async () => {
    await create(['target/media.jpg', 'links/file.xmp']);
    await fs.symlink(path.join(directory, 'target/media.jpg'), path.join(directory, 'links/file.jpg'));
    expect(await scan({ paths: [path.join(directory, 'links')] })).toEqual(new Map());
    expect(await scan({ paths: [path.join(directory, 'links')], followLinks: true, includeMetadata: true })).toEqual(
      new Map([[path.join(directory, 'links/file.jpg'), path.join(directory, 'links/file.xmp')]]),
    );
  });

  it('discovers excluded and hidden XMP names without importing orphan or filtered media', async () => {
    await create(['.hidden.jpg', '.hidden.xmp', 'file.jpg', 'file.xmp', 'orphan.xmp', 'ignored.txt', 'ignored.xmp']);
    expect(await scan({ includeHidden: true, exclusionPatterns: ['**/*.xmp'] })).toEqual(
      new Map([
        [path.join(directory, '.hidden.jpg'), path.join(directory, '.hidden.xmp')],
        [path.join(directory, 'file.jpg'), path.join(directory, 'file.xmp')],
      ]),
    );
    expect(await scan({ exclusionPatterns: ['**/*.jpg'] })).toEqual(new Map());
  });

  it('does not emit orphan-only directories', async () => {
    await create(['file.xmp', 'file.jpg.xmp', 'nested/orphan.xmp']);
    expect(await scan()).toEqual(new Map());
  });

  it('keeps parent summaries separate across directories and workers', async () => {
    const expected = new Map<string, SidecarResult>();
    for (let index = 0; index < 40; index++) {
      const names = [`${index}/file.jpg`, `${index}/nested/file.jpg`];
      if (index % 2 === 0) {
        names.push(`${index}/file.xmp`);
      }
      await create(names);
      expected.set(
        path.join(directory, `${index}/file.jpg`),
        index % 2 === 0 ? path.join(directory, `${index}/file.xmp`) : null,
      );
      expected.set(path.join(directory, `${index}/nested/file.jpg`), null);
    }
    expect(await scan({ threads: 4 })).toEqual(expected);
  });

  it('matches multi-dot names and JSON-escaped paths', async () => {
    await create(['edit.v2.jpg', 'edit.v2.xmp', String.raw`quoted"\雪.jpg`, String.raw`quoted"\雪.jpg.xmp`]);
    expect(await scan()).toEqual(
      new Map([
        [path.join(directory, 'edit.v2.jpg'), path.join(directory, 'edit.v2.xmp')],
        [path.join(directory, String.raw`quoted"\雪.jpg`), path.join(directory, String.raw`quoted"\雪.jpg.xmp`)],
      ]),
    );
  });

  it('uses the filesystem candidate lookup rules for uppercase XMP names', async () => {
    await create(['file.jpg', 'file.XMP']);
    const candidate = path.join(directory, 'file.xmp');
    const readable = await fs.access(candidate, fs.constants.R_OK).then(
      () => true,
      () => false,
    );
    expect(await scan()).toEqual(new Map([[path.join(directory, 'file.jpg'), readable ? candidate : null]]));
  });

  it('marks explicit media roots as unknown without sibling enumeration', async () => {
    await create(['file.jpg', 'file.jpg.xmp']);
    expect(await scan({ paths: [path.join(directory, 'file.jpg')] })).toEqual(
      new Map([[path.join(directory, 'file.jpg'), { status: 'unknown' }]]),
    );
  });

  it('keeps sidecars and metadata aligned across full and partial batches', async () => {
    const expected = new Map<string, SidecarResult>();
    for (let index = 0; index < 4101; index++) {
      await create([`${index}.jpg`, ...(index % 1000 === 0 ? [`${index}.jpg.xmp`] : [])]);
      expected.set(
        path.join(directory, `${index}.jpg`),
        index % 1000 === 0 ? path.join(directory, `${index}.jpg.xmp`) : null,
      );
    }
    expect(await scan({ threads: 1, includeMetadata: true })).toEqual(expected);
  });

  it('returns empty sidecar arrays for error-only batches', async () => {
    const batches = [];
    for await (const batch of walk({ paths: [path.join(directory, 'missing')], includeSidecars: true })) {
      batches.push(batch);
    }
    expect(batches).toHaveLength(1);
    expect(batches[0]).toMatchObject({ files: [], sidecars: [], size: null, modified: null });
    expect(batches[0].errors).toHaveLength(1);
  });

  it('does not allocate an enabled sidecar column by default', async () => {
    await create(['file.jpg', 'file.xmp']);
    for await (const batch of walk({ paths: [directory], extensions: ['jpg'] })) {
      expect(batch.sidecars).toBeNull();
    }
  });

  it('supports early termination and a subsequent scan', async () => {
    await create(Array.from({ length: 8200 }, (_, index) => `${index}.jpg`));
    for await (const batch of walk({ paths: [directory], includeSidecars: true, threads: 4 })) {
      expect(batch.files.length).toBeGreaterThan(0);
      break;
    }
    const found = await scan({ threads: 1 });
    expect(found.size).toBe(8200);
  });
});
