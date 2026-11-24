use super::*;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::time::{Duration, UNIX_EPOCH};

const PANIC_NAME: &str = "panic";

thread_local! {
  static SPAWNS_LEFT: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(super) fn inject_panic(path: &str) {
  assert!(
    Path::new(path).file_name().is_none_or(|name| name != PANIC_NAME),
    "Injected panic"
  );
}

pub(super) fn inject_spawn_failure() -> io::Result<()> {
  match SPAWNS_LEFT.get() {
    Some(0) => Err(io::Error::other("Injected spawn failure")),
    Some(left) => {
      SPAWNS_LEFT.set(Some(left - 1));
      Ok(())
    }
    None => Ok(()),
  }
}

struct TempDir(PathBuf);

impl TempDir {
  fn new(name: &str) -> Self {
    let path = std::env::temp_dir().join(format!("walkrs-ordered-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    Self(path)
  }

  fn write(&self, files: &[&str]) -> &Self {
    for file in files {
      let path = self.0.join(file);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, "").unwrap();
    }
    self
  }

  fn path(&self, relative: &str) -> String {
    self.0.join(relative).to_str().unwrap().to_string()
  }
}

impl Drop for TempDir {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

fn options(paths: Vec<String>) -> WalkOrderedOptions {
  WalkOrderedOptions {
    paths,
    include_hidden: None,
    exclusion_patterns: None,
    extensions: None,
    threads: Some(2.0),
  }
}

fn drain(walk: WalkOrdered) -> std::result::Result<usize, String> {
  let (done_tx, done_rx) = std_mpsc::channel();
  thread::spawn(move || {
    let mut batches = 0;
    while walk.rx.blocking_lock().blocking_recv().is_some() {
      batches += 1;
    }
    let _ = done_tx.send(walk.reader.end().map(|()| batches).map_err(|err| err.reason));
  });
  done_rx
    .recv_timeout(Duration::from_secs(10))
    .expect("walk did not terminate")
}

fn reader(paths: &[String]) -> DirectoryReader {
  DirectoryReader {
    selection: Selection::new(paths, None, None, None).unwrap(),
    window: 1,
    state: Mutex::default(),
    changed: Condvar::new(),
  }
}

fn names(entries: &[Entry]) -> Vec<&str> {
  entries.iter().map(|entry| &*entry.name).collect()
}

fn sorted(mut entries: Vec<Entry>) -> Vec<Box<str>> {
  entries.sort();
  entries.into_iter().map(|entry| entry.name).collect()
}

#[test]
fn sorts_a_file_before_a_directory_with_the_same_stem() {
  let entries = vec![Entry::directory("a"), Entry::file("a.jpg", 0)];

  assert_eq!(sorted(entries), ["a.jpg".into(), "a".into()]);
}

#[test]
fn sorts_a_directory_by_its_name_followed_by_a_slash() {
  let entries = vec![
    Entry::directory("a"),
    Entry::file("ab.jpg", 0),
    Entry::directory("a-b"),
    Entry::file("a.jpg", 0),
    Entry::directory("a b"),
    Entry::file("a0", 0),
  ];

  assert_eq!(
    sorted(entries),
    ["a b", "a-b", "a.jpg", "a", "a0", "ab.jpg"].map(Box::from)
  );
}

#[test]
fn sorts_names_in_utf8_byte_order() {
  let entries = vec![
    Entry::file("😀.jpg", 0),
    Entry::directory("～"),
    Entry::file("～.jpg", 0),
  ];

  assert_eq!(sorted(entries), ["～.jpg", "～", "😀.jpg"].map(Box::from));
}

#[test]
fn sorts_nested_directory_paths_after_their_parent() {
  let entries = vec![
    Entry::directory("/r/a/b"),
    Entry::directory("/r/a"),
    Entry::directory("/r/a.b"),
  ];

  assert_eq!(sorted(entries), ["/r/a.b", "/r/a", "/r/a/b"].map(Box::from));
}

#[test]
fn sorts_the_filesystem_root_before_its_hidden_children() {
  let entries = vec![Entry::directory("/.hidden"), Entry::directory("/")];

  assert_eq!(sorted(entries), ["/", "/.hidden"].map(Box::from));
}

#[test]
fn returns_a_name_listed_twice_once() {
  let mut listing = Listing {
    entries: vec![
      Entry::file("b.jpg", 0),
      Entry::directory("a"),
      Entry::file("b.jpg", 0),
      Entry::directory("a"),
    ],
    errors: Vec::new(),
  };

  listing.sort();

  assert_eq!(names(&listing.entries), ["a", "b.jpg"]);
}

#[test]
fn rounds_modified_times_like_node() {
  let after = |nanos: u64| UNIX_EPOCH + Duration::from_nanos(nanos);
  let before = |nanos: u64| UNIX_EPOCH - Duration::from_nanos(nanos);

  assert_eq!(epoch_millis(after(499_999)), 0);
  assert_eq!(epoch_millis(after(500_000)), 1);
  assert_eq!(epoch_millis(after(999_999_999)), 1000);
  assert_eq!(epoch_millis(after(1_700_000_000_123_456_789)), 1_700_000_000_123);
  assert_eq!(epoch_millis(after(1_700_000_000_123_499_900)), 1_700_000_000_124);
  assert_eq!(epoch_millis(before(1_000_000_500)), -1000);
  assert_eq!(epoch_millis(before(1_000_600_000)), -1001);
  assert_eq!(epoch_millis(before(500_000)), 0);
}

#[test]
fn clamps_the_thread_count() {
  assert_eq!(thread_count(Some(1.0)), 1);
  assert_eq!(thread_count(Some(1.9)), 1);
  assert_eq!(thread_count(Some(8.0)), 8);
  assert_eq!(thread_count(Some(64.0)), 64);
  assert_eq!(thread_count(Some(65.0)), 64);
  assert_eq!(thread_count(Some(1e9)), 64);
  assert_eq!(thread_count(Some(f64::INFINITY)), 64);
  for default in [
    None,
    Some(0.0),
    Some(0.5),
    Some(-1.0),
    Some(f64::NAN),
    Some(f64::NEG_INFINITY),
  ] {
    assert!((1..=64).contains(&thread_count(default)), "{default:?}");
  }
}

#[test]
fn stops_reading_ahead_once_the_buffered_entries_reach_the_limit() {
  let mut state = ReadState::default();
  state.pending.insert(Entry::directory("/r/a"));
  state.pending.insert(Entry::directory("/r/b"));
  state.pending.insert(Entry::directory("/r/c"));
  let wide = Listing {
    entries: (0..65_536).map(|i| Entry::file(format!("{i}.jpg"), 0)).collect(),
    errors: Vec::new(),
  };

  let directory = state.claim(100).unwrap();
  state.finish(directory, wide);

  assert_eq!(state.claim(100), None);
  state.wanted = Some("/r/c".to_string());
  assert_eq!(state.claim(100).as_deref(), Some("/r/c"));
  state.take("/r/a");
  assert_eq!(state.claim(100).as_deref(), Some("/r/b"));
}

#[test]
fn does_not_follow_a_directory_replaced_by_a_symlink() {
  let temp = TempDir::new("symlink");
  temp.write(&["target/a.jpg"]);
  std::os::unix::fs::symlink(temp.path("target"), temp.path("link")).unwrap();

  let listing = reader(&[]).read(&temp.path("link"));

  assert_eq!(names(&listing.entries), Vec::<&str>::new());
  assert_eq!(listing.errors.len(), 0);
}

#[test]
fn follows_a_root_that_is_a_symlink() {
  let temp = TempDir::new("symlink-root");
  temp.write(&["target/a.jpg"]);
  std::os::unix::fs::symlink(temp.path("target"), temp.path("link")).unwrap();

  let listing = reader(&[temp.path("link")]).read(&temp.path("link"));

  assert_eq!(names(&listing.entries), ["a.jpg"]);
}

#[test]
fn fails_the_walk_when_a_worker_panics() {
  let temp = TempDir::new("worker-panic");
  temp.write(&["a.jpg", "panic/b.jpg", "z.jpg"]);

  let result = drain(walk_ordered(options(vec![temp.path("")])).unwrap());

  assert!(result.is_err(), "{result:?}");
}

#[test]
fn fails_the_walk_when_the_emitter_panics() {
  let temp = TempDir::new("emitter-panic");
  temp.write(&["a.jpg", "panic", "z.jpg"]);

  let result = drain(walk_ordered(options(vec![temp.path("")])).unwrap());

  assert!(result.is_err(), "{result:?}");
}

#[test]
fn fails_to_start_when_a_thread_cannot_be_spawned() {
  let temp = TempDir::new("spawn-failure");
  temp.write(&["a.jpg"]);
  SPAWNS_LEFT.set(Some(1));

  let result = walk_ordered(options(vec![temp.path("")]));

  assert_eq!(result.err().map(|err| err.status), Some(Status::GenericFailure));
}

#[test]
fn completes_a_walk_without_failures() {
  let temp = TempDir::new("complete");
  temp.write(&["a.jpg", "b/c.jpg"]);

  let result = drain(walk_ordered(options(vec![temp.path("")])).unwrap());

  assert_eq!(result, Ok(1));
}
