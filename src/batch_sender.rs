use crate::WalkError;
use tokio::sync::mpsc::Sender;

const BATCH_SIZE: usize = 4096;
const BUF_CAPACITY: usize = BATCH_SIZE * 100;
const FILES_PREFIX: &[u8] = br#"{"files":["#;

pub(crate) struct BatchSender {
  files_count: usize,
  errors_count: usize,
  files: Vec<u8>,
  errors: Vec<u8>,
  tx: Sender<Vec<u8>>,
}

impl BatchSender {
  pub fn new(tx: Sender<Vec<u8>>) -> Self {
    let mut files = Vec::with_capacity(BUF_CAPACITY);
    files.extend_from_slice(FILES_PREFIX);
    Self {
      files_count: 0,
      errors_count: 0,
      files,
      errors: Vec::new(),
      tx,
    }
  }

  pub fn send_entry(&mut self, path: &str) -> Result<(), ()> {
    if self.files_count > 0 {
      self.files.push(b',');
    }

    // Serialize immediately
    serde_json::to_writer(&mut self.files, path).expect("Path serialization should never fail");
    self.files_count += 1;
    if self.files_count + self.errors_count >= BATCH_SIZE {
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
    if self.files_count + self.errors_count >= BATCH_SIZE {
      self.flush()?;
    }
    Ok(())
  }

  fn flush(&mut self) -> Result<(), ()> {
    if self.files_count + self.errors_count > 0 {
      // Merge file and error buffers
      self.files.extend_from_slice(br#"],"errors":["#);
      self.files.extend_from_slice(&self.errors);
      self.files.extend_from_slice(b"]}");
      let mut files = Vec::with_capacity(BUF_CAPACITY);
      files.extend_from_slice(FILES_PREFIX);
      let buf = std::mem::replace(&mut self.files, files);
      self.errors.clear();
      self.files_count = 0;
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
