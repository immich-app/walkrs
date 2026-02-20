mod batch_sender;
mod extension_filter;

use std::path::Path;
use std::sync::Arc;

use globset::{GlobSet, GlobSetBuilder};
use ignore::{DirEntry, WalkBuilder, WalkState};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{self, Sender};

use batch_sender::BatchSender;
use extension_filter::ExtensionFilter;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum WalkItem {
  #[serde(rename = "entry")]
  Entry { path: String },
  #[serde(rename = "error")]
  Error { path: Option<String>, message: String },
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

  std::thread::spawn(move || walker.run(|| visit(tx.clone(), Arc::clone(&exclusion_set), Arc::clone(&extension_set))));

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

fn visit(
  tx: Sender<Vec<u8>>,
  exclusion_set: Arc<GlobSet>,
  extension_filter: Arc<ExtensionFilter>,
) -> Box<dyn FnMut(std::result::Result<DirEntry, ignore::Error>) -> WalkState + Send> {
  let mut batch_sender = BatchSender::new(tx);

  Box::new(move |entry_result| {
    let entry = match entry_result {
      Ok(entry) => entry,
      Err(err) => {
        // Report the error and continue walking
        // The error message from ignore crate already includes the path
        let error = WalkItem::Error {
          path: None,
          message: err.to_string(),
        };
        if batch_sender.send(error).is_err() {
          return WalkState::Quit;
        }
        return WalkState::Continue;
      }
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

    let item = WalkItem::Entry {
      path: path_str.to_string(),
    };

    if batch_sender.send(item).is_err() {
      return WalkState::Quit;
    }

    WalkState::Continue
  })
}
