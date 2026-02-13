use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
}

#[napi(ts_return_type = "Promise<string[]>")]
pub async fn walk(options: WalkOptions) -> Result<Vec<String>> {
  if options.paths.is_empty() {
    return Ok(Vec::new());
  }

  let root_paths = options.paths.clone();
  let include_hidden = options.include_hidden.unwrap_or(false);
  let exclusion_patterns = options.exclusion_patterns.unwrap_or_default();
  let extensions = options.extensions.unwrap_or_default();

  tokio::task::spawn_blocking(move || find_files(&root_paths, include_hidden, &exclusion_patterns, &extensions))
    .await
    .map_err(|e| Error::new(Status::GenericFailure, format!("Task join error: {}", e)))?
}

struct BatchSender {
  batch: Vec<String>,
  tx: std::sync::mpsc::Sender<Vec<String>>,
  limit: usize,
}

impl BatchSender {
  fn new(tx: std::sync::mpsc::Sender<Vec<String>>, limit: usize) -> Self {
    Self {
      batch: Vec::with_capacity(limit),
      tx,
      limit,
    }
  }

  fn send(&mut self, item: String) -> std::result::Result<(), ()> {
    self.batch.push(item);
    if self.batch.len() >= self.limit {
      self.flush()?;
    }
    Ok(())
  }

  fn flush(&mut self) -> std::result::Result<(), ()> {
    if !self.batch.is_empty() {
      let batch = std::mem::replace(&mut self.batch, Vec::with_capacity(self.limit));
      self.tx.send(batch).map_err(|_| ())?;
    }
    Ok(())
  }
}

impl Drop for BatchSender {
  fn drop(&mut self) {
    let _ = self.flush();
  }
}

fn find_files(
  root_paths: &[String],
  include_hidden: bool,
  exclusion_patterns: &[String],
  extensions: &[String],
) -> Result<Vec<String>> {
  let mut exclusion_set = globset::GlobSetBuilder::new();
  for pattern in exclusion_patterns {
    if let Ok(glob) = globset::Glob::new(pattern) {
      exclusion_set.add(glob);
    }
  }
  let exclusion_set = match exclusion_set.build() {
    Ok(set) => set,
    Err(e) => {
      eprintln!("Error building exclusion patterns: {}", e);
      globset::GlobSetBuilder::new().build().unwrap()
    }
  };
  let exclusion_set = Arc::new(exclusion_set);

  let ext_set: std::collections::HashSet<String> = extensions
    .iter()
    .map(|ext| ext.strip_prefix('.').unwrap_or(ext).to_lowercase())
    .collect();
  let ext_set = Arc::new(ext_set);

  let (tx, rx) = std::sync::mpsc::channel::<Vec<String>>();
  let quit_flag = Arc::new(AtomicBool::new(false));

  let mut walker_builder = ignore::WalkBuilder::new(&root_paths[0]);
  for path in &root_paths[1..] {
    walker_builder.add(path);
  }

  let walker = walker_builder
    .git_ignore(false)
    .hidden(!include_hidden)
    .parents(false)
    .ignore(false)
    .git_global(false)
    .git_exclude(false)
    .build_parallel();

  walker.run(|| {
    let tx = tx.clone();
    let quit_flag = quit_flag.clone();
    let exclusion_set = Arc::clone(&exclusion_set);
    let ext_set = Arc::clone(&ext_set);
    let mut batch_sender = BatchSender::new(tx, 256);

    Box::new(move |entry_result| {
      if quit_flag.load(Ordering::Relaxed) {
        return ignore::WalkState::Quit;
      }

      let Ok(entry) = entry_result else {
        return ignore::WalkState::Continue;
      };

      let path = entry.path();

      if !exclusion_set.is_empty() && !exclusion_set.matches(path).is_empty() {
        return ignore::WalkState::Skip;
      }

      let Some(file_type) = entry.file_type() else {
        return ignore::WalkState::Continue;
      };

      if !file_type.is_file() {
        return ignore::WalkState::Continue;
      }

      if !ext_set.is_empty()
        && !path
          .extension()
          .and_then(|e| e.to_str())
          .is_some_and(|ext| ext_set.contains(&ext.to_lowercase()))
      {
        return ignore::WalkState::Continue;
      }

      if batch_sender.send(path.to_string_lossy().into_owned()).is_err() {
        quit_flag.store(true, Ordering::Relaxed);
        return ignore::WalkState::Quit;
      }
      ignore::WalkState::Continue
    })
  });

  drop(tx);

  let mut all_files = Vec::new();
  while let Ok(batch) = rx.recv() {
    all_files.extend(batch);
  }

  Ok(all_files)
}
