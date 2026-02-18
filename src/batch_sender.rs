use tokio::sync::mpsc::Sender;

const BATCH_SIZE: usize = 4096;
const BUF_CAPACITY: usize = BATCH_SIZE * 100;

pub(crate) struct BatchSender {
  count: usize,
  buf: Vec<u8>,
  tx: Sender<Vec<u8>>,
}

impl BatchSender {
  pub fn new(tx: Sender<Vec<u8>>) -> Self {
    let mut buf = Vec::with_capacity(BUF_CAPACITY);
    buf.push(b'[');
    Self { count: 0, buf, tx }
  }

  pub fn send(&mut self, item: &str) -> Result<(), ()> {
    if self.count > 0 {
      self.buf.push(b',');
    }
    serde_json::to_writer(&mut self.buf, item).unwrap();
    self.count += 1;
    if self.count >= BATCH_SIZE {
      self.flush()?;
    }
    Ok(())
  }

  fn flush(&mut self) -> Result<(), ()> {
    if self.count > 0 {
      self.buf.push(b']');
      let mut new_buf = Vec::with_capacity(BUF_CAPACITY);
      new_buf.push(b'[');
      let buf = std::mem::replace(&mut self.buf, new_buf);
      self.tx.blocking_send(buf).map_err(|_| ())?;
      self.count = 0;
    }
    Ok(())
  }
}

impl Drop for BatchSender {
  fn drop(&mut self) {
    let _ = self.flush();
  }
}
