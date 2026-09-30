use crate::WalkError;
use tokio::sync::mpsc::Sender;

const BATCH_SIZE: usize = 4096;
const BUF_CAPACITY: usize = BATCH_SIZE * 100;
const SIZE_CAPACITY: usize = BATCH_SIZE * 8;
const TIMESTAMP_CAPACITY: usize = BATCH_SIZE * 14;
const PATHS_PREFIX: &[u8] = br#"{"files":["#;

pub(crate) struct FileMetadata {
  pub size: u64,
  pub modified: i64,
  pub created: Option<i64>,
}

struct MetadataBuffers {
  size: Vec<u8>,
  modified: Vec<u8>,
  created: Vec<u8>,
}

pub(crate) struct BatchSender {
  paths_count: usize,
  errors_count: usize,
  paths: Vec<u8>,
  errors: Vec<u8>,
  metadata: Option<MetadataBuffers>,
  tx: Sender<Vec<u8>>,
}

impl BatchSender {
  fn new_paths(include_metadata: bool) -> Vec<u8> {
    // The final payload contains both paths and metadata columns.
    let capacity = BUF_CAPACITY
      + if include_metadata {
        SIZE_CAPACITY + 2 * TIMESTAMP_CAPACITY
      } else {
        0
      };
    let mut paths = Vec::with_capacity(capacity);
    paths.extend_from_slice(PATHS_PREFIX);
    paths
  }

  pub fn new(tx: Sender<Vec<u8>>, include_metadata: bool) -> Self {
    Self {
      paths_count: 0,
      errors_count: 0,
      paths: Self::new_paths(include_metadata),
      errors: Vec::new(),
      metadata: include_metadata.then(|| MetadataBuffers {
        size: Vec::with_capacity(SIZE_CAPACITY),
        modified: Vec::with_capacity(TIMESTAMP_CAPACITY),
        created: Vec::with_capacity(TIMESTAMP_CAPACITY),
      }),
      tx,
    }
  }

  pub fn send_entry(&mut self, path: &str, metadata: Option<FileMetadata>) -> Result<(), ()> {
    debug_assert_eq!(self.metadata.is_some(), metadata.is_some());
    if let (Some(buffers), Some(metadata)) = (&mut self.metadata, metadata) {
      if self.paths_count > 0 {
        buffers.size.push(b',');
        buffers.modified.push(b',');
        buffers.created.push(b',');
      }
      serde_json::to_writer(&mut buffers.size, &metadata.size).expect("Integer serialization should never fail");
      serde_json::to_writer(&mut buffers.modified, &metadata.modified)
        .expect("Integer serialization should never fail");
      serde_json::to_writer(&mut buffers.created, &metadata.created).expect("Integer serialization should never fail");
    }
    if self.paths_count > 0 {
      self.paths.push(b',');
    }

    // Serialize immediately
    serde_json::to_writer(&mut self.paths, path).expect("Path serialization should never fail");
    self.paths_count += 1;
    if self.paths_count + self.errors_count >= BATCH_SIZE {
      self.flush()?;
    }
    Ok(())
  }

  pub fn send_error(&mut self, error: WalkError) -> Result<(), ()> {
    if self.errors_count > 0 {
      self.errors.push(b',');
    }

    // Serialize immediately
    serde_json::to_writer(&mut self.errors, &error).expect("WalkError serialization should never fail");
    self.errors_count += 1;
    if self.paths_count + self.errors_count >= BATCH_SIZE {
      self.flush()?;
    }
    Ok(())
  }

  fn flush(&mut self) -> Result<(), ()> {
    if self.paths_count + self.errors_count > 0 {
      // Larger paths, numbers or errors may exceed our estimates. Reserve the
      // whole suffix once to avoid repeated growth while merging the buffers.
      let metadata_len =
        self
          .metadata
          .as_ref()
          .map_or(br#"],"size":null,"modified":null,"created":null"#.len(), |metadata| {
            br#"],"size":["#.len()
              + metadata.size.len()
              + br#"],"modified":["#.len()
              + metadata.modified.len()
              + br#"],"created":["#.len()
              + metadata.created.len()
              + 1
          });
      self
        .paths
        .reserve_exact(metadata_len + br#","errors":["#.len() + self.errors.len() + b"]}".len());
      // Merge file and error buffers
      if let Some(metadata) = &mut self.metadata {
        self.paths.extend_from_slice(br#"],"size":["#);
        self.paths.extend_from_slice(&metadata.size);
        self.paths.extend_from_slice(br#"],"modified":["#);
        self.paths.extend_from_slice(&metadata.modified);
        self.paths.extend_from_slice(br#"],"created":["#);
        self.paths.extend_from_slice(&metadata.created);
        self.paths.push(b']');
        metadata.size.clear();
        metadata.modified.clear();
        metadata.created.clear();
      } else {
        self
          .paths
          .extend_from_slice(br#"],"size":null,"modified":null,"created":null"#);
      }
      self.paths.extend_from_slice(br#","errors":["#);
      self.paths.extend_from_slice(&self.errors);
      self.paths.extend_from_slice(b"]}");
      let paths = Self::new_paths(self.metadata.is_some());
      let buf = std::mem::replace(&mut self.paths, paths);
      self.errors.clear();
      self.paths_count = 0;
      self.errors_count = 0;
      self.tx.blocking_send(buf).map_err(|_| ())?;
    }
    Ok(())
  }
}

impl Drop for BatchSender {
  fn drop(&mut self) {
    let _ = self.flush();
  }
}

#[cfg(test)]
mod tests;
