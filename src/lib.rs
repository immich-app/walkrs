mod batch_sender;
mod extension_filter;
mod sidecars;

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use globset::{GlobSet, GlobSetBuilder};
use ignore::{DirEntry, ParallelVisitor, ParallelVisitorBuilder, WalkBuilder, WalkState};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{self, Sender};

use batch_sender::{BatchSender, FileMetadata};
use extension_filter::ExtensionFilter;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct WalkError {
  pub path: Option<String>,
  pub message: String,
}

#[napi(object)]
pub struct WalkOptions {
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

  #[napi(ts_type = "boolean | undefined")]
  pub include_metadata: Option<bool>,

  #[napi(ts_type = "boolean | undefined")]
  pub include_sidecars: Option<bool>,

  #[napi(ts_type = "boolean | undefined")]
  pub follow_links: Option<bool>,
}

#[napi(async_iterator)]
pub struct Walk {
  rx: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

#[napi]
impl AsyncGenerator for Walk {
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
      let mut rx = rx.lock().await;
      rx.close();
      while rx.try_recv().is_ok() {}
      Ok(None)
    }
  }
}

#[napi]
pub fn walk(options: WalkOptions) -> Result<Walk> {
  const CHANNEL_CAPACITY: usize = 16;
  let (tx, rx) = mpsc::channel::<Vec<u8>>(CHANNEL_CAPACITY);

  if options.paths.is_empty() {
    return Ok(Walk {
      rx: Arc::new(Mutex::new(rx)),
    });
  }

  let exclusion_set = Arc::new(build_exclusion_set(&options.exclusion_patterns.unwrap_or_default())?);
  let extension_set = Arc::new(ExtensionFilter::new(&options.extensions.unwrap_or_default()));

  let mut walk_builder = WalkBuilder::new(&options.paths[0]);
  for path in &options.paths[1..] {
    walk_builder.add(path);
  }

  let threads = options.threads.unwrap_or(0);

  walk_builder
    .git_ignore(false)
    .hidden(!options.include_hidden.unwrap_or(false))
    .parents(false)
    .ignore(false)
    .threads(threads as usize)
    .git_global(false)
    .git_exclude(false);
  walk_builder.follow_links(options.follow_links.unwrap_or(false));

  let walker = walk_builder.build_parallel();
  let include_metadata = options.include_metadata.unwrap_or(false);
  let include_sidecars = options.include_sidecars.unwrap_or(false);

  std::thread::spawn(move || {
    walker.visit(&mut VisitorBuilder {
      tx,
      exclusion_set,
      extension_filter: extension_set,
      include_metadata,
      include_sidecars,
    });
  });

  Ok(Walk {
    rx: Arc::new(Mutex::new(rx)),
  })
}

fn build_exclusion_set(exclusion_patterns: &[String]) -> Result<GlobSet> {
  let mut builder = GlobSetBuilder::new();
  for pattern in exclusion_patterns {
    builder.add(
      globset::GlobBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .map_err(|e| {
          Error::new(
            Status::InvalidArg,
            format!("Invalid exclusion pattern '{pattern}': {e}"),
          )
        })?,
    );
  }
  builder
    .build()
    .map_err(|e| Error::new(Status::InvalidArg, format!("Failed to build exclusion patterns: {e}")))
}

// JSON numbers must remain exact when parsed as JavaScript numbers.
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

fn timestamp_millis(time: SystemTime) -> std::result::Result<i64, &'static str> {
  let (duration, sign) = match time.duration_since(UNIX_EPOCH) {
    Ok(duration) => (duration, 1),
    Err(error) => (error.duration(), -1),
  };
  let millis = duration.as_millis();
  if millis > u128::from(MAX_SAFE_INTEGER) {
    return Err("Modification time exceeds the JavaScript safe integer range");
  }
  Ok((millis as i64) * sign)
}

fn read_metadata(entry: &DirEntry) -> std::result::Result<FileMetadata, String> {
  // The parallel walker follows root symlinks for their file type, but its
  // entry metadata can still describe the link. Match the root's target type.
  let metadata = if entry.depth() == 0 {
    std::fs::metadata(entry.path()).map_err(|error| format!("Failed to read metadata: {error}"))?
  } else {
    entry
      .metadata()
      .map_err(|error| format!("Failed to read metadata: {error}"))?
  };
  let size = metadata.len();
  if size > MAX_SAFE_INTEGER {
    return Err("File size exceeds the JavaScript safe integer range".into());
  }
  let modified = metadata
    .modified()
    .map_err(|error| format!("Failed to read modification time: {error}"))?;
  let modified = timestamp_millis(modified).map_err(str::to_owned)?;
  Ok(FileMetadata { size, modified })
}

struct VisitorBuilder {
  tx: Sender<Vec<u8>>,
  exclusion_set: Arc<GlobSet>,
  extension_filter: Arc<ExtensionFilter>,
  include_metadata: bool,
  include_sidecars: bool,
}

impl<'s> ParallelVisitorBuilder<'s> for VisitorBuilder {
  fn build(&mut self) -> Box<dyn ParallelVisitor + 's> {
    Box::new(Visitor {
      batch_sender: BatchSender::new(self.tx.clone(), self.include_metadata, self.include_sidecars),
      exclusion_set: Arc::clone(&self.exclusion_set),
      extension_filter: Arc::clone(&self.extension_filter),
      include_metadata: self.include_metadata,
      include_sidecars: self.include_sidecars,
      sidecar_index: self.include_sidecars.then(|| Box::new(sidecars::DirectoryIndex::new())),
    })
  }
}

struct Visitor {
  batch_sender: BatchSender,
  exclusion_set: Arc<GlobSet>,
  extension_filter: Arc<ExtensionFilter>,
  include_metadata: bool,
  include_sidecars: bool,
  sidecar_index: Option<Box<sidecars::DirectoryIndex>>,
}

impl ParallelVisitor for Visitor {
  fn directory_summary(&mut self, entries: &[std::fs::DirEntry], complete: bool) -> u8 {
    self
      .sidecar_index
      .as_mut()
      .map_or(sidecars::UNKNOWN, |index| index.summarize(entries, complete))
  }

  fn entry_summary(&mut self, entry: &DirEntry, summary: u8) -> u8 {
    match &self.sidecar_index {
      Some(index)
        if entry.file_type().is_some_and(|ft| ft.is_file()) && self.extension_filter.is_match(entry.path()) =>
      {
        index.entry_summary(entry.file_name().as_encoded_bytes(), summary)
      }
      _ => summary,
    }
  }

  fn visit(&mut self, entry_result: std::result::Result<DirEntry, ignore::Error>) -> WalkState {
    if self.batch_sender.is_closed() {
      return WalkState::Quit;
    }
    let entry = match entry_result {
      Ok(entry) => entry,
      Err(err) => {
        // Report the error and continue walking
        // The error message from ignore crate already includes the path
        let error = WalkError {
          path: None,
          message: err.to_string(),
        };
        if self.batch_sender.send_error(error).is_err() {
          return WalkState::Quit;
        }
        return WalkState::Continue;
      }
    };

    let Some(ft) = entry.file_type() else {
      return WalkState::Continue;
    };

    let path: &Path = entry.path();

    if self.exclusion_set.is_match(path) {
      return if ft.is_dir() {
        WalkState::Skip
      } else {
        WalkState::Continue
      };
    }

    if !ft.is_file() {
      return WalkState::Continue;
    }

    if !self.extension_filter.is_match(path) {
      return WalkState::Continue;
    }

    let Some(path_str) = path.to_str() else {
      return WalkState::Continue;
    };

    let metadata = if self.include_metadata {
      match read_metadata(&entry) {
        Ok(metadata) => Some(metadata),
        Err(message) => {
          if self
            .batch_sender
            .send_error(WalkError {
              path: Some(path_str.into()),
              message,
            })
            .is_err()
          {
            return WalkState::Quit;
          }
          return WalkState::Continue;
        }
      }
    } else {
      None
    };

    let sidecar = self
      .include_sidecars
      .then(|| sidecars::resolve(path_str, entry.parent_summary()));
    if self.batch_sender.send_entry(path_str, metadata, sidecar).is_err() {
      return WalkState::Quit;
    }

    WalkState::Continue
  }
}

#[cfg(test)]
mod tests;
