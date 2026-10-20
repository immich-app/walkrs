use super::*;
use std::time::Duration;

#[test]
fn modification_times_are_signed_integer_milliseconds() {
  assert_eq!(timestamp_millis(UNIX_EPOCH), Ok(0));
  assert_eq!(
    timestamp_millis(UNIX_EPOCH + Duration::from_micros(1_234_567)),
    Ok(1234)
  );
  assert_eq!(
    timestamp_millis(UNIX_EPOCH - Duration::from_micros(1_234_567)),
    Ok(-1234)
  );
  assert_eq!(timestamp_millis(UNIX_EPOCH - Duration::from_micros(999)), Ok(0));
}

#[test]
fn modification_times_outside_the_safe_integer_range_are_errors() {
  assert!(timestamp_millis(UNIX_EPOCH + Duration::from_millis(MAX_SAFE_INTEGER + 1)).is_err());
  assert!(timestamp_millis(UNIX_EPOCH - Duration::from_millis(MAX_SAFE_INTEGER + 1)).is_err());
}

#[cfg(unix)]
#[test]
fn explicit_file_symlinks_use_target_metadata() {
  use std::fs::{self, File, FileTimes};
  use std::os::unix::fs::symlink;
  use std::path::PathBuf;
  use std::sync::Mutex;

  struct TestDirectory(PathBuf);
  impl Drop for TestDirectory {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.0);
    }
  }

  let directory = TestDirectory(std::env::temp_dir().join(format!(
    "walkrs-root-symlink-{}-{}",
    std::process::id(),
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
  )));
  fs::create_dir(&directory.0).unwrap();
  let target = directory.0.join("target.jpg");
  let link = directory.0.join("link.jpg");
  fs::write(&target, vec![0; 1000]).unwrap();
  let modified = UNIX_EPOCH + Duration::from_millis(1_700_000_000_123);
  File::open(&target)
    .unwrap()
    .set_times(FileTimes::new().set_modified(modified))
    .unwrap();
  symlink(&target, &link).unwrap();
  let target_metadata = fs::metadata(&target).unwrap();
  let link_metadata = fs::symlink_metadata(&link).unwrap();
  assert_ne!(target_metadata.len(), link_metadata.len());
  assert_ne!(target_metadata.modified().unwrap(), link_metadata.modified().unwrap());

  let results = Mutex::new(Vec::new());
  WalkBuilder::new(&link).threads(1).build_parallel().run(|| {
    Box::new(|entry| {
      let entry = entry.unwrap();
      assert_eq!(entry.depth(), 0);
      assert!(entry.file_type().unwrap().is_file());
      let metadata = read_metadata(&entry).unwrap();
      results
        .lock()
        .unwrap()
        .push((metadata.size, metadata.modified, metadata.created));
      WalkState::Continue
    })
  });
  assert_eq!(
    results.into_inner().unwrap(),
    vec![(
      target_metadata.len(),
      timestamp_millis(target_metadata.modified().unwrap()).unwrap(),
      target_metadata
        .created()
        .ok()
        .map(timestamp_millis)
        .transpose()
        .unwrap()
    )]
  );
}
