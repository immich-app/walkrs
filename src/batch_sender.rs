use crate::{WalkBatch, WalkError};
use tokio::sync::mpsc::Sender;

const BATCH_SIZE: usize = 4096;
const BUF_CAPACITY: usize = BATCH_SIZE * 100;

pub(crate) struct BatchSender {
  files_count: usize,
  errors_count: usize,
  files: Vec<String>,
  errors: Vec<WalkError>,
  total_items: usize,
  tx: Sender<Vec<u8>>,
}

impl BatchSender {
  pub fn new(tx: Sender<Vec<u8>>) -> Self {
    let mut buf = Vec::with_capacity(BUF_CAPACITY);
    buf.push(b'[');
    Self {
      files_count: 0,
      errors_count: 0,
      files: Vec::new(),
      errors: Vec::new(),
      total_items: 0,
      tx,
    }
  }

  pub fn send_entry(&mut self, path: String) -> Result<(), ()> {
    self.files.push(path);
    self.files_count += 1;
    self.total_items += 1;
    if self.total_items >= BATCH_SIZE {
      self.flush()?;
    }
    Ok(())
  }

  pub fn send_error(&mut self, error: WalkError) -> Result<(), ()> {
    self.errors.push(error);
    self.errors_count += 1;
    self.total_items += 1;
    if self.total_items >= BATCH_SIZE {
      self.flush()?;
    }
    Ok(())
  }

  fn flush(&mut self) -> Result<(), ()> {
    if self.total_items > 0 {
      let batch = WalkBatch {
        files: std::mem::take(&mut self.files),
        errors: std::mem::take(&mut self.errors),
      };
      let buf = serde_json::to_vec(&batch).expect("WalkBatch serialization should never fail");
      self.tx.blocking_send(buf).map_err(|_| ())?;
      self.files_count = 0;
      self.errors_count = 0;
      self.total_items = 0;
    }
    Ok(())
  }
}

impl Drop for BatchSender {
  fn drop(&mut self) {
    let _ = self.flush();
  }
}
