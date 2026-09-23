use super::*;
use serde_json::{Value, json};
use tokio::sync::mpsc::{channel, error::TryRecvError};

#[test]
fn empty_sender_does_not_send_a_batch() {
  let (tx, mut rx) = channel(1);
  let mut sender = BatchSender::new(tx, false);
  sender.flush().unwrap();
  drop(sender);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn escapes_interleaved_files_and_errors() {
  let (tx, mut rx) = channel(1);
  let mut sender = BatchSender::new(tx, false);
  let path = "photos/\"quoted\"\\newline\n\t雪.jpg";
  let message = "failed: \"quoted\"\\newline\n\t\0雪";
  sender
    .send_error(WalkError {
      path: None,
      message: message.into(),
    })
    .unwrap();
  sender.send_entry(path, None).unwrap();
  sender
    .send_error(WalkError {
      path: Some(path.into()),
      message: message.into(),
    })
    .unwrap();
  sender.send_entry("", None).unwrap();
  drop(sender);

  let batch: Value = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
  assert_eq!(
    batch,
    json!({
      "files": [path, ""],
      "size": null,
      "modified": null,
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
      let mut sender = BatchSender::new(tx, false);
      for i in 0..total {
        if mode == 1 || (mode == 2 && i % 2 == 0) {
          sender
            .send_error(WalkError {
              path: None,
              message: i.to_string(),
            })
            .unwrap();
        } else {
          sender.send_entry(&i.to_string(), None).unwrap();
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
        assert_eq!(
          batch,
          json!({"files": files, "size": null, "modified": null, "errors": errors})
        );
      }
      assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
    }
  }
}

#[test]
fn closed_receiver_returns_an_error() {
  for last_item_is_error in [false, true] {
    let (tx, rx) = channel(1);
    let mut sender = BatchSender::new(tx, false);
    drop(rx);
    for _ in 0..BATCH_SIZE - 1 {
      sender.send_entry("file.jpg", None).unwrap();
    }
    let result = if last_item_is_error {
      sender.send_error(WalkError {
        path: None,
        message: "error".into(),
      })
    } else {
      sender.send_entry("file.jpg", None)
    };
    assert_eq!(result, Err(()));
    assert_eq!(sender.flush(), Ok(()));
  }
}

#[test]
fn metadata_columns_stay_aligned_across_errors_and_batch_boundaries() {
  let (tx, mut rx) = channel(3);
  let mut sender = BatchSender::new(tx, true);
  let total = 2 * BATCH_SIZE + 3;
  for i in 0..total {
    if i % 3 == 0 {
      sender
        .send_error(WalkError {
          path: None,
          message: i.to_string(),
        })
        .unwrap();
    } else {
      sender
        .send_entry(
          &i.to_string(),
          Some(FileMetadata {
            size: i as u64,
            modified: -(i as i64),
          }),
        )
        .unwrap();
    }
  }
  drop(sender);
  for start in (0..total).step_by(BATCH_SIZE) {
    let end = (start + BATCH_SIZE).min(total);
    let entries: Vec<_> = (start..end).filter(|i| i % 3 != 0).collect();
    let errors: Vec<_> = (start..end)
      .filter(|i| i % 3 == 0)
      .map(|i| json!({"path": null, "message": i.to_string()}))
      .collect();
    let batch: Value = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
    assert_eq!(
      batch,
      json!({
        "files": entries.iter().map(usize::to_string).collect::<Vec<_>>(),
        "size": entries,
        "modified": entries.iter().map(|&i| -(i as i64)).collect::<Vec<_>>(),
        "errors": errors,
      })
    );
  }
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[test]
fn errors_only_metadata_batch_has_empty_columns() {
  let (tx, mut rx) = channel(1);
  let mut sender = BatchSender::new(tx, true);
  sender
    .send_error(WalkError {
      path: Some("missing".into()),
      message: "not found".into(),
    })
    .unwrap();
  drop(sender);
  let batch: Value = serde_json::from_slice(&rx.try_recv().unwrap()).unwrap();
  assert_eq!(
    batch,
    json!({ "files": [], "size": [], "modified": [], "errors": [{ "path": "missing", "message": "not found" }] })
  );
}

#[test]
fn metadata_disabled_does_not_allocate_column_buffers() {
  let (tx, _rx) = channel(1);
  assert!(BatchSender::new(tx, false).metadata.is_none());
}
