mod batch_sender;
mod entry_filter;
mod extension_filter;
mod walk_ordered;

use std::path::Path;
use std::sync::Arc;

use ignore::{DirEntry, WalkBuilder, WalkState};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::sync::Mutex;
use tokio::sync::mpsc::{self, Sender};

use batch_sender::BatchSender;
use entry_filter::EntryFilter;
pub use walk_ordered::{WalkOrdered, WalkOrderedOptions, walk_ordered};

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

  let filter = Arc::new(EntryFilter::new(
    &options.exclusion_patterns.unwrap_or_default(),
    &options.extensions.unwrap_or_default(),
  )?);

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

  std::thread::spawn(move || walker.run(|| visit(tx.clone(), Arc::clone(&filter))));

  Ok(Walk {
    rx: Arc::new(Mutex::new(rx)),
  })
}

fn visit(
  tx: Sender<Vec<u8>>,
  filter: Arc<EntryFilter>,
) -> Box<dyn FnMut(std::result::Result<DirEntry, ignore::Error>) -> WalkState + Send> {
  let mut batch_sender = BatchSender::new(tx);

  Box::new(move |entry_result| {
    let entry = match entry_result {
      Ok(entry) => entry,
      Err(err) => {
        let error = WalkError {
          path: None,
          message: err.to_string(),
        };
        if batch_sender.send_error(error).is_err() {
          return WalkState::Quit;
        }
        return WalkState::Continue;
      }
    };

    let Some(ft) = entry.file_type() else {
      return WalkState::Continue;
    };

    let path: &Path = entry.path();

    if ft.is_dir() {
      let skipped = if entry.depth() == 0 {
        filter.skips_root(path)
      } else {
        filter.skips_directory(path)
      };
      return if skipped { WalkState::Skip } else { WalkState::Continue };
    }

    if !ft.is_file() || !filter.includes_file(path) {
      return WalkState::Continue;
    }

    let Some(path_str) = path.to_str() else {
      return WalkState::Continue;
    };

    if batch_sender.send_entry(path_str).is_err() {
      return WalkState::Quit;
    }

    WalkState::Continue
  })
}
