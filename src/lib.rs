mod batch_sender;
mod extension_filter;

use std::path::Path;
use std::sync::Arc;

use globset::{GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Serialize;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{self, Sender};

use batch_sender::BatchSender;
use extension_filter::ExtensionFilter;

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

  let walker = walk_builder.build_parallel();
  let include_metadata = options.include_metadata.unwrap_or(false);

  std::thread::spawn(move || {
    walker.run(|| {
      visit(
        tx.clone(),
        Arc::clone(&exclusion_set),
        Arc::clone(&extension_set),
        include_metadata,
      )
    })
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

#[derive(Serialize)]
pub struct FileEntry {
  pub path: String,
  pub modified: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub created: Option<String>,
}

fn visit(
  tx: Sender<Vec<u8>>,
  exclusion_set: Arc<GlobSet>,
  extension_filter: Arc<ExtensionFilter>,
  include_metadata: bool,
) -> Box<dyn FnMut(std::result::Result<DirEntry, ignore::Error>) -> WalkState + Send> {
  let mut batch_sender = BatchSender::new(tx);

  Box::new(move |entry_result| {
    let Ok(entry) = entry_result else {
      return WalkState::Continue;
    };

    let Some(ft) = entry.file_type() else {
      return WalkState::Continue;
    };

    let path: &Path = entry.path();

    if exclusion_set.is_match(path) {
      return if ft.is_dir() {
        WalkState::Skip
      } else {
        WalkState::Continue
      };
    }

    if !ft.is_file() {
      return WalkState::Continue;
    }

    if !extension_filter.is_match(path) {
      return WalkState::Continue;
    }

    let Some(path_str) = path.to_str() else {
      return WalkState::Continue;
    };

    if include_metadata {
      if let Ok(metadata) = entry.metadata() {
        let modified = metadata
          .modified()
          .unwrap_or_else(|e| panic!("Failed to read modified time for {path_str}: {e}"))
          .duration_since(std::time::UNIX_EPOCH)
          .unwrap_or_else(|e| {
            panic!("Failed to convert modified time for {path_str} to unix time: {e}")
          })
          .as_secs()
          .to_string();
        let created = metadata
          .created()
          .ok()
          .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
          .map(|d| d.as_secs().to_string());

        let file_entry = FileEntry {
          path: path_str.to_string(),
          modified,
          created,
        };
        if batch_sender.send(&file_entry).is_err() {
          return WalkState::Quit;
        }
      }
    } else if batch_sender.send(&path_str).is_err() {
      return WalkState::Quit;
    }

    WalkState::Continue
  })
}
