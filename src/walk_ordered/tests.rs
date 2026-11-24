use super::*;
use std::time::{Duration, UNIX_EPOCH};

fn sorted(mut entries: Vec<Entry>) -> Vec<String> {
  entries.sort();
  entries.into_iter().map(|entry| entry.path).collect()
}

#[test]
fn sorts_a_file_before_a_directory_with_the_same_stem() {
  let entries = vec![Entry::directory("/r/a".into()), Entry::file("/r/a.jpg".into(), 0)];

  assert_eq!(sorted(entries), ["/r/a.jpg", "/r/a"]);
}

#[test]
fn sorts_a_directory_by_its_name_followed_by_a_slash() {
  let entries = vec![
    Entry::directory("/r/a".into()),
    Entry::file("/r/ab.jpg".into(), 0),
    Entry::directory("/r/a-b".into()),
    Entry::file("/r/a.jpg".into(), 0),
    Entry::directory("/r/a b".into()),
    Entry::file("/r/a0".into(), 0),
  ];

  assert_eq!(
    sorted(entries),
    ["/r/a b", "/r/a-b", "/r/a.jpg", "/r/a", "/r/a0", "/r/ab.jpg"]
  );
}

#[test]
fn sorts_names_in_utf8_byte_order() {
  let entries = vec![
    Entry::file("/r/😀.jpg".into(), 0),
    Entry::directory("/r/～".into()),
    Entry::file("/r/～.jpg".into(), 0),
  ];

  assert_eq!(sorted(entries), ["/r/～.jpg", "/r/～", "/r/😀.jpg"]);
}

#[test]
fn rounds_modified_times_to_the_nearest_millisecond_half_up() {
  let after = |nanos: u64| UNIX_EPOCH + Duration::from_nanos(nanos);
  let before = |nanos: u64| UNIX_EPOCH - Duration::from_nanos(nanos);

  assert_eq!(epoch_millis(after(499_999)), 0);
  assert_eq!(epoch_millis(after(500_000)), 1);
  assert_eq!(epoch_millis(after(999_999_999)), 1000);
  assert_eq!(epoch_millis(after(1_700_000_000_123_456_789)), 1_700_000_000_123);
  assert_eq!(epoch_millis(before(1_000_000_500)), -1000);
  assert_eq!(epoch_millis(before(1_000_600_000)), -1001);
  assert_eq!(epoch_millis(before(500_000)), 0);
}

#[test]
fn normalizes_paths_lexically() {
  let cases = [
    ("/lib", "/lib"),
    ("/lib/", "/lib"),
    ("/lib//", "/lib"),
    ("//lib/./sub/../", "/lib"),
    ("/lib/.", "/lib"),
    ("/..", "/"),
    ("/", "/"),
    ("lib/../..", ".."),
    ("./lib", "lib"),
    ("", "."),
  ];

  for (path, expected) in cases {
    assert_eq!(normalize(path), expected, "{path}");
  }
}

#[test]
fn sorts_the_filesystem_root_before_its_hidden_children() {
  let entries = vec![Entry::directory("/.hidden".into()), Entry::directory("/".into())];

  assert_eq!(sorted(entries), ["/", "/.hidden"]);
}
