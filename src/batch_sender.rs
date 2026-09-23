use crate::WalkError;
use tokio::sync::mpsc::Sender;

const BATCH_SIZE: usize = 4096;
const BUF_CAPACITY: usize = BATCH_SIZE * 100;
const PATHS_PREFIX: &[u8] = br#"{"files":["#;

pub(crate) struct FileMetadata {
  pub size: u64,
  pub modified: i64,
}

struct MetadataBuffers {
  size: Vec<u8>,
  modified: Vec<u8>,
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
  pub fn new(tx: Sender<Vec<u8>>, include_metadata: bool) -> Self {
    let mut paths = Vec::with_capacity(BUF_CAPACITY);
    paths.extend_from_slice(PATHS_PREFIX);
    Self {
      paths_count: 0,
      errors_count: 0,
      paths,
      errors: Vec::new(),
      metadata: include_metadata.then(|| MetadataBuffers {
        size: Vec::with_capacity(BATCH_SIZE * 8),
        modified: Vec::with_capacity(BATCH_SIZE * 14),
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
      }
      serde_json::to_writer(&mut buffers.size, &metadata.size).expect("Integer serialization should never fail");
      serde_json::to_writer(&mut buffers.modified, &metadata.modified)
        .expect("Integer serialization should never fail");
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
      // Merge file and error buffers
      if let Some(metadata) = &mut self.metadata {
        self.paths.extend_from_slice(br#"],"size":["#);
        self.paths.extend_from_slice(&metadata.size);
        self.paths.extend_from_slice(br#"],"modified":["#);
        self.paths.extend_from_slice(&metadata.modified);
        self.paths.push(b']');
        metadata.size.clear();
        metadata.modified.clear();
      } else {
        self.paths.extend_from_slice(br#"],"size":null,"modified":null"#);
      }
      self.paths.extend_from_slice(br#","errors":["#);
      self.paths.extend_from_slice(&self.errors);
      self.paths.extend_from_slice(b"]}");
      let mut paths = Vec::with_capacity(BUF_CAPACITY);
      paths.extend_from_slice(PATHS_PREFIX);
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
