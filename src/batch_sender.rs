use tokio::sync::mpsc::UnboundedSender;

const BATCH_SIZE: usize = 4096;

pub(crate) struct BatchSender {
  batch: Vec<String>,
  buf: Vec<u8>,
  tx: UnboundedSender<Vec<u8>>,
}

impl BatchSender {
  pub fn new(tx: UnboundedSender<Vec<u8>>) -> Self {
    Self {
      batch: Vec::with_capacity(BATCH_SIZE),
      buf: Vec::new(),
      tx,
    }
  }

  pub fn send(&mut self, item: String) -> Result<(), ()> {
    self.batch.push(item);
    if self.batch.len() >= BATCH_SIZE {
      self.flush()?;
    }
    Ok(())
  }

  fn flush(&mut self) -> Result<(), ()> {
    if !self.batch.is_empty() {
      serde_json::to_writer(&mut self.buf, &self.batch).unwrap();
      self.tx.send(self.buf.clone()).map_err(|_| ())?;
      self.buf.clear();
      self.batch.clear();
    }
    Ok(())
  }
}

impl Drop for BatchSender {
  fn drop(&mut self) {
    let _ = self.flush();
  }
}
