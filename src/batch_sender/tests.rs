use super::*;
use serde_json::{Value, json};
use tokio::sync::mpsc::{channel, error::TryRecvError};

#[test]
fn empty_sender_does_not_send_a_batch() {
  let (tx, mut rx) = channel(1);
  let mut sender = BatchSender::new(tx);
  sender.flush().unwrap();
  drop(sender);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn escapes_interleaved_files_and_errors() {
  let (tx, mut rx) = channel(1);
  let mut sender = BatchSender::new(tx);
  let path = "photos/\"quoted\"\\newline\n\t雪.jpg";
  let message = "failed: \"quoted\"\\newline\n\t\0雪";
  sender
    .send_error(WalkError {
      path: None,
      message: message.into(),
    })
    .unwrap();
  sender.send_entry(path).unwrap();
  sender
    .send_error(WalkError {
      path: Some(path.into()),
      message: message.into(),
    })
    .unwrap();
  sender.send_entry("").unwrap();
  drop(sender);

  let batch: Value = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
  assert_eq!(
    batch,
    json!({
      "files": [path, ""],
      "errors": [{"path": null, "message": message}, {"path": path, "message": message}]
    })
  );
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn batches_files_errors_and_mixed_items_at_the_combined_limit() {
  for total in [BATCH_SIZE - 1, BATCH_SIZE, BATCH_SIZE + 1, 2 * BATCH_SIZE + 1] {
    for mode in 0..3 {
      let (tx, mut rx) = channel(3);
      let mut sender = BatchSender::new(tx);
      for i in 0..total {
        if mode == 1 || (mode == 2 && i % 2 == 0) {
          sender
            .send_error(WalkError {
              path: None,
              message: i.to_string(),
            })
            .unwrap();
        } else {
          sender.send_entry(&i.to_string()).unwrap();
        }
        assert_eq!(rx.len(), (i + 1) / BATCH_SIZE);
      }
      drop(sender);

      for start in (0..total).step_by(BATCH_SIZE) {
        let end = (start + BATCH_SIZE).min(total);
        let mut files = Vec::new();
        let mut errors = Vec::new();
        for i in start..end {
          if mode == 1 || (mode == 2 && i % 2 == 0) {
            errors.push(json!({"path": null, "message": i.to_string()}));
          } else {
            files.push(i.to_string());
          }
        }
        let batch: Value = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(batch, json!({"files": files, "errors": errors}));
      }
      assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }
  }
}

#[test]
fn closed_receiver_returns_an_error() {
  for last_item_is_error in [false, true] {
    let (tx, rx) = channel(1);
    let mut sender = BatchSender::new(tx);
    drop(rx);
    for _ in 0..BATCH_SIZE - 1 {
      sender.send_entry("file.jpg").unwrap();
    }
    let result = if last_item_is_error {
      sender.send_error(WalkError {
        path: None,
        message: "error".into(),
      })
    } else {
      sender.send_entry("file.jpg")
    };
    assert_eq!(result, Err(()));
    assert_eq!(sender.flush(), Ok(()));
  }
}
