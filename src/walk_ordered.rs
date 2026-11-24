use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io;
use std::iter::Peekable;
use std::num::NonZero;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use std::vec;

use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::mpsc;

use crate::WalkError;
use crate::batch_sender::BatchSender;
use crate::entry_filter::EntryFilter;

const CHANNEL_CAPACITY: usize = 16;
const READ_AHEAD_PER_THREAD: usize = 4;
const NANOS_PER_MILLI: i128 = 1_000_000;

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
  pub threads: Option<u32>,
}

#[napi(async_iterator)]
pub struct WalkOrdered {
  rx: Arc<tokio::sync::Mutex<mpsc::Receiver<Vec<u8>>>>,
}

#[napi]
impl AsyncGenerator for WalkOrdered {
  type Yield = Buffer;
  type Next = ();
  type Return = ();

  fn next(&mut self, _value: Option<Self::Next>) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let rx = Arc::clone(&self.rx);
    async move { Ok(rx.lock().await.recv().await.map(Into::into)) }
  }

  fn complete(
    &mut self,
    _value: Option<Self::Return>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let rx = Arc::clone(&self.rx);
    async move {
      rx.lock().await.close();
      Ok(None)
    }
  }
}

#[napi]
pub fn walk_ordered(options: WalkOrderedOptions) -> Result<WalkOrdered> {
  let filter = EntryFilter::new(
    &options.exclusion_patterns.unwrap_or_default(),
    &options.extensions.unwrap_or_default(),
  )?;
  let threads = match options.threads {
    Some(threads) if threads > 0 => threads as usize,
    _ => thread::available_parallelism().map_or(1, NonZero::get),
  };
  let reader = Arc::new(DirectoryReader {
    filter,
    include_hidden: options.include_hidden.unwrap_or(false),
    window: threads * READ_AHEAD_PER_THREAD,
    state: Mutex::default(),
    changed: Condvar::new(),
  });
  let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

  for _ in 0..threads {
    let reader = Arc::clone(&reader);
    thread::spawn(move || reader.run());
  }
  thread::spawn(move || {
    let _ = emit(&reader, &options.paths, &mut BatchSender::with_modified(tx));
    reader.close();
  });

  Ok(WalkOrdered {
    rx: Arc::new(tokio::sync::Mutex::new(rx)),
  })
}

fn emit(reader: &DirectoryReader, paths: &[String], sender: &mut BatchSender) -> std::result::Result<(), ()> {
  let mut walk = OrderedWalk {
    reader,
    roots: reader.roots(paths, sender)?.into_iter().peekable(),
    stack: Vec::new(),
  };

  while let Some(entry) = walk.next() {
    match entry.kind {
      EntryKind::File { modified } => sender.send_file(&entry.path, modified)?,
      EntryKind::Directory => {
        let listing = reader.take(entry.path);
        for error in listing.errors {
          sender.send_error(error)?;
        }
        walk.stack.push(listing.entries.into_iter());
      }
    }
  }
  Ok(())
}

struct OrderedWalk<'a> {
  reader: &'a DirectoryReader,
  roots: Peekable<vec::IntoIter<Entry>>,
  stack: Vec<vec::IntoIter<Entry>>,
}

impl OrderedWalk<'_> {
  fn next(&mut self) -> Option<Entry> {
    while self.stack.last().is_some_and(|entries| entries.len() == 0) {
      self.stack.pop();
    }

    let listed = self.stack.last().and_then(|entries| entries.as_slice().first());
    match self.roots.peek().map(|root| listed.map(|listed| root.cmp(listed))) {
      Some(None | Some(Ordering::Less)) => self.next_root(),
      Some(Some(Ordering::Equal)) => {
        self.roots.next();
        self.stack.last_mut()?.next()
      }
      None | Some(Some(Ordering::Greater)) => self.stack.last_mut()?.next(),
    }
  }

  fn next_root(&mut self) -> Option<Entry> {
    let root = self.roots.next()?;
    if let EntryKind::Directory = root.kind {
      self.reader.schedule(Entry::directory(root.path.clone()));
    }
    Some(root)
  }
}

#[derive(Debug)]
pub(crate) enum EntryKind {
  File { modified: i64 },
  Directory,
}

#[derive(Debug)]
pub(crate) struct Entry {
  pub path: String,
  pub kind: EntryKind,
}

impl Entry {
  pub fn file(path: String, modified: i64) -> Self {
    Self {
      path,
      kind: EntryKind::File { modified },
    }
  }

  pub fn directory(path: String) -> Self {
    Self {
      path,
      kind: EntryKind::Directory,
    }
  }

  fn sort_key(&self) -> impl Iterator<Item = u8> + '_ {
    let separator = (matches!(self.kind, EntryKind::Directory) && !self.path.ends_with('/')).then_some(b'/');
    self.path.bytes().chain(separator)
  }
}

impl Ord for Entry {
  fn cmp(&self, other: &Self) -> Ordering {
    self.sort_key().cmp(other.sort_key())
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

#[derive(Default)]
struct ReadState {
  pending: BTreeSet<Entry>,
  in_flight: usize,
  ready: HashMap<String, Listing>,
  wanted: Option<String>,
  closed: bool,
}

impl ReadState {
  fn claim(&mut self, window: usize) -> Option<String> {
    if let Some(wanted) = &self.wanted
      && let Some(entry) = self.pending.take(&Entry::directory(wanted.clone()))
    {
      return Some(entry.path);
    }

    if self.in_flight + self.ready.len() >= window {
      return None;
    }

    self.pending.pop_first().map(|entry| entry.path)
  }
}

struct DirectoryReader {
  filter: EntryFilter,
  include_hidden: bool,
  window: usize,
  state: Mutex<ReadState>,
  changed: Condvar,
}

impl DirectoryReader {
  fn roots(&self, paths: &[String], sender: &mut BatchSender) -> std::result::Result<Vec<Entry>, ()> {
    let mut roots = Vec::new();
    for path in paths.iter().map(|path| normalize(path)).collect::<BTreeSet<_>>() {
      match self.read_root(path) {
        Ok(Some(root)) => roots.push(root),
        Ok(None) => {}
        Err(error) => sender.send_error(error)?,
      }
    }
    roots.sort_unstable();
    Ok(roots)
  }

  fn read_root(&self, path: String) -> std::result::Result<Option<Entry>, WalkError> {
    let metadata = fs::metadata(&path).map_err(|err| walk_error(&path, err))?;

    if metadata.is_file() {
      if !self.filter.includes_file(Path::new(&path)) {
        return Ok(None);
      }
      let modified = metadata.modified().map_err(|err| walk_error(&path, err))?;
      return Ok(Some(Entry::file(path, epoch_millis(modified))));
    }

    Ok((metadata.is_dir() && !self.filter.skips_root(Path::new(&path))).then(|| Entry::directory(path)))
  }

  fn run(&self) {
    while let Some(directory) = self.claim() {
      let listing = self.read(&directory);
      self.finish(directory, listing);
    }
  }

  fn schedule(&self, directory: Entry) {
    self.lock().pending.insert(directory);
    self.changed.notify_all();
  }

  fn take(&self, directory: String) -> Listing {
    let mut state = self.lock();
    state.wanted = Some(directory.clone());
    self.changed.notify_all();
    loop {
      if let Some(listing) = state.ready.remove(&directory) {
        state.wanted = None;
        self.changed.notify_all();
        return listing;
      }
      state = self.changed.wait(state).unwrap();
    }
  }

  fn close(&self) {
    self.lock().closed = true;
    self.changed.notify_all();
  }

  fn claim(&self) -> Option<String> {
    let mut state = self.lock();
    loop {
      if state.closed {
        return None;
      }
      if let Some(directory) = state.claim(self.window) {
        state.in_flight += 1;
        return Some(directory);
      }
      state = self.changed.wait(state).unwrap();
    }
  }

  fn finish(&self, directory: String, listing: Listing) {
    let mut state = self.lock();
    state.in_flight -= 1;
    for entry in &listing.entries {
      if let EntryKind::Directory = entry.kind {
        state.pending.insert(Entry::directory(entry.path.clone()));
      }
    }
    state.ready.insert(directory, listing);
    self.changed.notify_all();
  }

  fn read(&self, directory: &str) -> Listing {
    let mut listing = Listing::default();
    let dir_entries = match fs::read_dir(directory) {
      Ok(dir_entries) => dir_entries,
      Err(err) => {
        listing.errors.push(walk_error(directory, err));
        return listing;
      }
    };

    for dir_entry in dir_entries {
      let result = dir_entry
        .map_err(|err| walk_error(directory, err))
        .and_then(|dir_entry| self.read_entry(directory, &dir_entry));
      match result {
        Ok(Some(entry)) => listing.entries.push(entry),
        Ok(None) => {}
        Err(error) => listing.errors.push(error),
      }
    }

    listing.entries.sort_unstable();
    listing
  }

  fn read_entry(&self, directory: &str, dir_entry: &fs::DirEntry) -> std::result::Result<Option<Entry>, WalkError> {
    let Ok(name) = dir_entry.file_name().into_string() else {
      return Ok(None);
    };

    if !self.include_hidden && name.starts_with('.') {
      return Ok(None);
    }

    let path = join(directory, &name);
    let file_type = dir_entry.file_type().map_err(|err| walk_error(&path, err))?;

    if file_type.is_dir() {
      return Ok((!self.filter.skips_directory(Path::new(&path))).then(|| Entry::directory(path)));
    }

    if !file_type.is_file() || !self.filter.includes_file(Path::new(&path)) {
      return Ok(None);
    }

    let modified = dir_entry
      .metadata()
      .and_then(|metadata| metadata.modified())
      .map_err(|err| walk_error(&path, err))?;
    Ok(Some(Entry::file(path, epoch_millis(modified))))
  }

  fn lock(&self) -> MutexGuard<'_, ReadState> {
    self.state.lock().unwrap()
  }
}

fn join(directory: &str, name: &str) -> String {
  if directory.ends_with('/') {
    format!("{directory}{name}")
  } else {
    format!("{directory}/{name}")
  }
}

fn normalize(path: &str) -> String {
  let absolute = path.starts_with('/');
  let mut components: Vec<&str> = Vec::new();
  for component in path.split('/') {
    match component {
      "" | "." => {}
      ".." if components.last().is_some_and(|last| *last != "..") => {
        components.pop();
      }
      ".." if absolute => {}
      component => components.push(component),
    }
  }

  let joined = components.join("/");
  match (absolute, joined.is_empty()) {
    (true, _) => format!("/{joined}"),
    (false, true) => ".".to_string(),
    (false, false) => joined,
  }
}

fn walk_error(path: &str, err: io::Error) -> WalkError {
  WalkError {
    path: Some(path.to_string()),
    message: err.to_string(),
  }
}

fn epoch_millis(time: SystemTime) -> i64 {
  let nanos = match time.duration_since(UNIX_EPOCH) {
    Ok(duration) => duration.as_nanos() as i128,
    Err(err) => -(err.duration().as_nanos() as i128),
  };
  (nanos + NANOS_PER_MILLI / 2).div_euclid(NANOS_PER_MILLI) as i64
}

#[cfg(test)]
mod tests;
