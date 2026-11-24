use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::iter::Peekable;
use std::num::NonZero;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::mpsc;

use crate::WalkError;
use crate::batch_sender::BatchSender;
use crate::selection::{Node, Selection};

const CHANNEL_CAPACITY: usize = 16;
const READ_AHEAD_PER_THREAD: usize = 4;
const READ_AHEAD_ENTRIES: usize = 65_536;
const MAX_THREADS: usize = 64;
const MILLIS_PER_SECOND: f64 = 1e3;
const NANOS_PER_MILLI: f64 = 1e6;
const NANOS_PER_SECOND: u32 = 1_000_000_000;

#[napi(object)]
pub struct WalkOrderedOptions {
  #[napi(ts_type = "string[]")]
  pub paths: Vec<String>,

  #[napi(ts_type = "boolean | undefined")]
  pub include_hidden: Option<bool>,

  #[napi(ts_type = "string[] | undefined")]
  pub exclusion_patterns: Option<Vec<String>>,

  #[napi(ts_type = "string[] | undefined")]
  pub extensions: Option<Vec<String>>,

  #[napi(ts_type = "number | undefined")]
  pub threads: Option<f64>,
}

#[napi(async_iterator)]
pub struct WalkOrdered {
  rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>>,
  reader: Arc<DirectoryReader>,
}

#[napi]
impl AsyncGenerator for WalkOrdered {
  type Yield = Buffer;
  type Next = ();
  type Return = ();

  fn next(&mut self, _value: Option<Self::Next>) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let rx = Arc::clone(&self.rx);
    let reader = Arc::clone(&self.reader);
    async move {
      match rx.lock().await.recv().await {
        Some(batch) => Ok(Some(batch.into())),
        None => reader.end().map(|()| None),
      }
    }
  }

  fn complete(
    &mut self,
    _value: Option<Self::Return>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let rx = Arc::clone(&self.rx);
    self.reader.close();
    async move {
      rx.lock().await.close();
      Ok(None)
    }
  }
}

#[napi]
pub fn walk_ordered(options: WalkOrderedOptions) -> Result<WalkOrdered> {
  let selection = Selection::new(
    &options.paths,
    options.include_hidden,
    options.exclusion_patterns,
    options.extensions,
  )?;
  let threads = thread_count(options.threads);
  let reader = Arc::new(DirectoryReader {
    selection,
    window: threads * READ_AHEAD_PER_THREAD,
    state: Mutex::default(),
    changed: Condvar::new(),
  });
  let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

  if let Err(err) = start(&reader, threads, tx) {
    reader.close();
    return Err(err);
  }

  Ok(WalkOrdered {
    rx: Arc::new(tokio::sync::Mutex::new(rx)),
    reader,
  })
}

fn thread_count(requested: Option<f64>) -> usize {
  match requested {
    Some(threads) if threads >= 1.0 => threads.min(MAX_THREADS as f64) as usize,
    _ => thread::available_parallelism().map_or(1, NonZero::get).min(MAX_THREADS),
  }
}

fn start(reader: &Arc<DirectoryReader>, threads: usize, tx: mpsc::Sender<Vec<u8>>) -> Result<()> {
  for _ in 0..threads {
    spawn(reader, DirectoryReader::run)?;
  }
  spawn(reader, move |reader| {
    let mut sender = BatchSender::with_modified(tx);
    let _ = emit(reader, &mut sender);
    reader.finish();
  })
}

fn spawn(reader: &Arc<DirectoryReader>, body: impl FnOnce(&DirectoryReader) + Send + 'static) -> Result<()> {
  let spawn_error = |err| Error::new(Status::GenericFailure, format!("Failed to spawn walker thread: {err}"));

  #[cfg(test)]
  tests::inject_spawn_failure().map_err(spawn_error)?;

  let reader = Arc::clone(reader);
  thread::Builder::new()
    .spawn(move || {
      if let Err(panic) = panic::catch_unwind(AssertUnwindSafe(|| body(&reader))) {
        reader.fail(panic_message(&*panic));
      }
    })
    .map(drop)
    .map_err(spawn_error)
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
  let reason = panic
    .downcast_ref::<&str>()
    .copied()
    .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
    .unwrap_or("unknown panic");
  format!("Walker thread panicked: {reason}")
}

fn emit(reader: &DirectoryReader, sender: &mut BatchSender) -> std::result::Result<(), ()> {
  let mut walk = OrderedWalk {
    reader,
    roots: reader.roots(sender)?.into_iter().peekable(),
    stack: Vec::new(),
    path: String::new(),
  };

  while let Some(kind) = walk.next() {
    match kind {
      EntryKind::File { modified } => {
        #[cfg(test)]
        tests::inject_panic(&walk.path);

        sender.send_file(&walk.path, modified)?;
      }
      EntryKind::Directory => {
        let Some(listing) = reader.take(&walk.path) else {
          return Ok(());
        };
        for error in listing.errors {
          sender.send_error(error)?;
        }
        walk.stack.push(Level {
          directory: walk.path.clone(),
          entries: listing.entries.into_iter(),
        });
      }
    }
  }
  Ok(())
}

struct OrderedWalk<'a> {
  reader: &'a DirectoryReader,
  roots: Peekable<vec::IntoIter<Entry>>,
  stack: Vec<Level>,
  path: String,
}

struct Level {
  directory: String,
  entries: vec::IntoIter<Entry>,
}

impl OrderedWalk<'_> {
  fn next(&mut self) -> Option<EntryKind> {
    while self.stack.last().is_some_and(|level| level.entries.len() == 0) {
      self.stack.pop();
    }

    let listed = self.stack.last().and_then(Level::peek_key);
    match self
      .roots
      .peek()
      .map(|root| listed.map(|listed| root.sort_key().cmp(listed)))
    {
      Some(None | Some(Ordering::Less)) => self.next_root(),
      Some(Some(Ordering::Equal)) => {
        self.roots.next();
        self.next_listed()
      }
      None | Some(Some(Ordering::Greater)) => self.next_listed(),
    }
  }

  fn next_root(&mut self) -> Option<EntryKind> {
    let root = self.roots.next()?;
    if let EntryKind::Directory = root.kind {
      self.reader.schedule(Entry::directory(root.name.clone()));
    }
    self.path.clear();
    self.path.push_str(&root.name);
    Some(root.kind)
  }

  fn next_listed(&mut self) -> Option<EntryKind> {
    let level = self.stack.last_mut()?;
    let entry = level.entries.next()?;
    self.path.clear();
    push_child(&mut self.path, &level.directory, &entry.name);
    Some(entry.kind)
  }
}

impl Level {
  fn peek_key(&self) -> Option<impl Iterator<Item = u8> + '_> {
    let entry = self.entries.as_slice().first()?;
    let separator = (!self.directory.ends_with('/')).then_some(b'/');
    Some(self.directory.bytes().chain(separator).chain(entry.sort_key()))
  }
}

#[derive(Debug)]
pub(crate) enum EntryKind {
  File { modified: i64 },
  Directory,
}

#[derive(Debug)]
pub(crate) struct Entry {
  pub name: Box<str>,
  pub kind: EntryKind,
}

impl Entry {
  pub fn file(name: impl Into<Box<str>>, modified: i64) -> Self {
    Self {
      name: name.into(),
      kind: EntryKind::File { modified },
    }
  }

  pub fn directory(name: impl Into<Box<str>>) -> Self {
    Self {
      name: name.into(),
      kind: EntryKind::Directory,
    }
  }

  fn sort_key(&self) -> impl Iterator<Item = u8> + '_ {
    self.sort_key_from(0)
  }

  fn sort_key_from(&self, start: usize) -> impl Iterator<Item = u8> + '_ {
    let separator = (matches!(self.kind, EntryKind::Directory) && !self.name.ends_with('/')).then_some(b'/');
    self.name.as_bytes()[start..].iter().copied().chain(separator)
  }
}

impl Ord for Entry {
  fn cmp(&self, other: &Self) -> Ordering {
    let common = self.name.len().min(other.name.len());
    self.name.as_bytes()[..common]
      .cmp(&other.name.as_bytes()[..common])
      .then_with(|| self.sort_key_from(common).cmp(other.sort_key_from(common)))
  }
}

impl PartialOrd for Entry {
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl PartialEq for Entry {
  fn eq(&self, other: &Self) -> bool {
    self.cmp(other) == Ordering::Equal
  }
}

impl Eq for Entry {}

#[derive(Default)]
struct Listing {
  entries: Vec<Entry>,
  errors: Vec<WalkError>,
}

impl Listing {
  fn sort(&mut self) {
    self.entries.sort_unstable();
    self.entries.dedup();
  }
}

#[derive(Default)]
struct ReadState {
  pending: BTreeSet<Entry>,
  in_flight: usize,
  ready: HashMap<String, Listing>,
  buffered: usize,
  wanted: Option<String>,
  closed: bool,
  finished: bool,
  failure: Option<String>,
}

impl ReadState {
  fn claim(&mut self, window: usize) -> Option<String> {
    let directory = self.next_pending(window)?;
    self.in_flight += 1;
    Some(directory)
  }

  fn next_pending(&mut self, window: usize) -> Option<String> {
    if let Some(wanted) = &self.wanted
      && let Some(entry) = self.pending.take(&Entry::directory(wanted.as_str()))
    {
      return Some(entry.name.into());
    }

    if self.in_flight + self.ready.len() >= window || self.buffered >= READ_AHEAD_ENTRIES {
      return None;
    }

    self.pending.pop_first().map(|entry| entry.name.into())
  }

  fn finish(&mut self, directory: String, listing: Listing) {
    self.in_flight -= 1;
    let mut path = String::new();
    for entry in &listing.entries {
      if let EntryKind::Directory = entry.kind {
        path.clear();
        push_child(&mut path, &directory, &entry.name);
        self.pending.insert(Entry::directory(path.as_str()));
      }
    }
    self.buffered += listing.entries.len();
    self.ready.insert(directory, listing);
  }

  fn take(&mut self, directory: &str) -> Option<Listing> {
    let listing = self.ready.remove(directory)?;
    self.buffered -= listing.entries.len();
    Some(listing)
  }
}

struct DirectoryReader {
  selection: Selection,
  window: usize,
  state: Mutex<ReadState>,
  changed: Condvar,
}

impl DirectoryReader {
  fn roots(&self, sender: &mut BatchSender) -> std::result::Result<Vec<Entry>, ()> {
    let mut roots = Vec::new();
    for path in &self.selection.paths {
      match self.read_root(path) {
        Ok(Some(root)) => roots.push(root),
        Ok(None) => {}
        Err(error) => sender.send_error(error)?,
      }
    }
    roots.sort_unstable();
    Ok(roots)
  }

  fn read_root(&self, path: &str) -> std::result::Result<Option<Entry>, WalkError> {
    let metadata = fs::metadata(path).map_err(|err| walk_error(path, err))?;

    if metadata.is_file() {
      if !self.selection.includes_root(path, Node::File) {
        return Ok(None);
      }
      let modified = metadata.modified().map_err(|err| walk_error(path, err))?;
      return Ok(Some(Entry::file(path, epoch_millis(modified))));
    }

    Ok((metadata.is_dir() && self.selection.includes_root(path, Node::Directory)).then(|| Entry::directory(path)))
  }

  fn run(&self) {
    while let Some(directory) = self.claim() {
      let listing = self.read(&directory);
      self.lock().finish(directory, listing);
      self.changed.notify_all();
    }
  }

  fn schedule(&self, directory: Entry) {
    self.lock().pending.insert(directory);
    self.changed.notify_all();
  }

  fn take(&self, directory: &str) -> Option<Listing> {
    let mut state = self.lock();
    state.wanted = Some(directory.to_string());
    self.changed.notify_all();
    loop {
      if state.closed {
        return None;
      }
      if let Some(listing) = state.take(directory) {
        state.wanted = None;
        self.changed.notify_all();
        return Some(listing);
      }
      state = self.wait(state);
    }
  }

  fn finish(&self) {
    let mut state = self.lock();
    state.finished = true;
    state.closed = true;
    self.changed.notify_all();
  }

  fn close(&self) {
    self.lock().closed = true;
    self.changed.notify_all();
  }

  fn fail(&self, message: String) {
    let mut state = self.lock();
    state.failure.get_or_insert(message);
    state.closed = true;
    self.changed.notify_all();
  }

  fn end(&self) -> Result<()> {
    let state = self.lock();
    match (&state.failure, state.finished) {
      (Some(failure), _) => Err(Error::from_reason(failure.clone())),
      (None, false) => Err(Error::from_reason("Walk ended unexpectedly")),
      (None, true) => Ok(()),
    }
  }

  fn claim(&self) -> Option<String> {
    let mut state = self.lock();
    loop {
      if state.closed {
        return None;
      }
      if let Some(directory) = state.claim(self.window) {
        return Some(directory);
      }
      state = self.wait(state);
    }
  }

  fn read(&self, directory: &str) -> Listing {
    #[cfg(test)]
    tests::inject_panic(directory);

    let mut listing = Listing::default();
    let dir_entries = match self.open(directory) {
      Ok(Some(dir_entries)) => dir_entries,
      Ok(None) => return listing,
      Err(err) => {
        listing.errors.push(walk_error(directory, err));
        return listing;
      }
    };

    let mut path = String::new();
    for dir_entry in dir_entries {
      path.clear();
      push_child(&mut path, directory, "");
      let result = dir_entry
        .map_err(|err| walk_error(directory, err))
        .and_then(|dir_entry| self.read_entry(&mut path, &dir_entry));
      match result {
        Ok(Some(entry)) => listing.entries.push(entry),
        Ok(None) => {}
        Err(error) => listing.errors.push(error),
      }
    }

    listing.sort();
    listing
  }

  fn open(&self, directory: &str) -> io::Result<Option<fs::ReadDir>> {
    if self.selection.paths.contains(directory) {
      return fs::read_dir(directory).map(Some);
    }
    read_dir_without_following(directory)
  }

  fn read_entry(&self, path: &mut String, dir_entry: &fs::DirEntry) -> std::result::Result<Option<Entry>, WalkError> {
    let file_name = dir_entry.file_name();
    if !self.selection.includes_name(file_name.as_encoded_bytes()) {
      return Ok(None);
    }

    let name = file_name.to_string_lossy();
    path.push_str(&name);
    let file_type = dir_entry.file_type().map_err(|err| walk_error(path, err))?;

    let node = match (file_type.is_dir(), file_type.is_file()) {
      (true, _) => Node::Directory,
      (_, true) => Node::File,
      _ => return Ok(None),
    };
    if !self.selection.includes_child(path, node) {
      return Ok(None);
    }

    let Cow::Borrowed(name) = name else {
      return Err(WalkError {
        path: Some(path.clone()),
        message: "Path is not valid UTF-8".to_string(),
      });
    };

    if file_type.is_dir() {
      return Ok(Some(Entry::directory(name)));
    }

    let modified = dir_entry
      .metadata()
      .and_then(|metadata| metadata.modified())
      .map_err(|err| walk_error(path, err))?;
    Ok(Some(Entry::file(name, epoch_millis(modified))))
  }

  fn lock(&self) -> MutexGuard<'_, ReadState> {
    self.state.lock().unwrap_or_else(PoisonError::into_inner)
  }

  fn wait<'a>(&self, state: MutexGuard<'a, ReadState>) -> MutexGuard<'a, ReadState> {
    self.changed.wait(state).unwrap_or_else(PoisonError::into_inner)
  }
}

#[cfg(unix)]
fn read_dir_without_following(directory: &str) -> io::Result<Option<fs::ReadDir>> {
  use std::os::fd::AsRawFd;
  use std::os::unix::fs::MetadataExt;

  let handle = fs::File::open(directory)?;
  let opened = handle.metadata()?;
  let linked = fs::symlink_metadata(directory)?;
  if !linked.is_dir() || (linked.dev(), linked.ino()) != (opened.dev(), opened.ino()) {
    return Ok(None);
  }
  fs::read_dir(format!("/dev/fd/{}", handle.as_raw_fd())).map(Some)
}

#[cfg(not(unix))]
fn read_dir_without_following(directory: &str) -> io::Result<Option<fs::ReadDir>> {
  match fs::symlink_metadata(directory)?.is_dir() {
    true => fs::read_dir(directory).map(Some),
    false => Ok(None),
  }
}

fn push_child(path: &mut String, directory: &str, name: &str) {
  path.push_str(directory);
  if !directory.ends_with('/') {
    path.push('/');
  }
  path.push_str(name);
}

fn walk_error(path: &str, err: io::Error) -> WalkError {
  WalkError {
    path: Some(path.to_string()),
    message: err.to_string(),
  }
}

fn epoch_millis(time: SystemTime) -> i64 {
  let (seconds, nanos) = match time.duration_since(UNIX_EPOCH) {
    Ok(after) => (after.as_secs() as f64, after.subsec_nanos()),
    Err(err) => match err.duration() {
      before if before.subsec_nanos() == 0 => (-(before.as_secs() as f64), 0),
      before => (
        -(before.as_secs() as f64) - 1.0,
        NANOS_PER_SECOND - before.subsec_nanos(),
      ),
    },
  };
  round_half_up(seconds * MILLIS_PER_SECOND + f64::from(nanos) / NANOS_PER_MILLI) as i64
}

fn round_half_up(value: f64) -> f64 {
  let ceil = value.ceil();
  if ceil - 0.5 > value { ceil - 1.0 } else { ceil }
}

#[cfg(test)]
mod tests;
