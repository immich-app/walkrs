use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::selection::{ROOT, Selection};

#[napi(object)]
pub struct MatcherOptions {
  #[napi(ts_type = "string[] | undefined")]
  pub paths: Option<Vec<String>>,

  #[napi(ts_type = "boolean | undefined")]
  pub include_hidden: Option<bool>,

  #[napi(ts_type = "string[] | undefined")]
  pub exclusion_patterns: Option<Vec<String>>,

  #[napi(ts_type = "string[] | undefined")]
  pub extensions: Option<Vec<String>>,
}

#[napi]
pub struct Matcher {
  selection: Selection,
}

#[napi]
impl Matcher {
  #[napi(constructor)]
  pub fn new(options: MatcherOptions) -> Result<Self> {
    let paths = options.paths.unwrap_or_else(|| vec![ROOT.to_string()]);
    Ok(Self {
      selection: Selection::new(
        &paths,
        options.include_hidden,
        options.exclusion_patterns,
        options.extensions,
      )?,
    })
  }

  #[napi]
  pub fn includes(&self, path: String) -> bool {
    self.selection.includes(&path)
  }

  #[napi]
  pub fn includes_all(&self, paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|path| self.selection.includes(path)).collect()
  }
}
