import type { Buffer } from 'node:buffer';

// This file is a hack to work around https://github.com/napi-rs/napi-rs/issues/3122

declare module '@immich/walkrs' {
  interface Walk extends AsyncIterable<Buffer> {
    [Symbol.asyncIterator](): AsyncIterator<Buffer>;
  }
}
