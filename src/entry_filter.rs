use std::borrow::Cow;
use std::path::{Path, PathBuf};

use globset::{Candidate, GlobSet, GlobSetBuilder};
use napi::bindgen_prelude::*;

use crate::extension_filter::ExtensionFilter;
use crate::pattern;

pub(crate) struct EntryFilter {
  exclusion_set: GlobSet,
  pruning_set: GlobSet,
  extension_filter: ExtensionFilter,
}

impl EntryFilter {
  pub fn new(exclusion_patterns: &[String], extensions: &[String]) -> Result<Self> {
    let directory_patterns = exclusion_patterns
      .iter()
      .filter_map(|pattern| directory_pattern(pattern).map(String::from));
    let pruning_patterns: Vec<String> = exclusion_patterns.iter().cloned().chain(directory_patterns).collect();
    Ok(Self {
      exclusion_set: build_exclusion_set(exclusion_patterns)?,
      pruning_set: build_exclusion_set(&pruning_patterns)?,
      extension_filter: ExtensionFilter::new(extensions),
    })
  }

  pub fn skips_root(&self, path: &Path) -> bool {
    let folded = fold_path(path);
    self.exclusion_set.is_match_candidate(&Candidate::new(&folded))
  }

  pub fn skips_directory(&self, path: &Path) -> bool {
    let folded = fold_path(path);
    self.pruning_set.is_match_candidate(&Candidate::new(&folded))
  }

  pub fn includes_file(&self, path: &Path) -> bool {
    self.extension_filter.is_match(path) && !self.skips_directory(path)
  }
}

fn directory_pattern(pattern: &str) -> Option<&str> {
  let directory = pattern.strip_suffix("/**")?;
  Some(directory_pattern(directory).unwrap_or(directory))
}

fn fold_path(path: &Path) -> Cow<'_, Path> {
  match path.to_str().map(pattern::fold_case) {
    Some(Cow::Owned(folded)) => Cow::Owned(PathBuf::from(folded)),
    _ => Cow::Borrowed(path),
  }
}

fn build_exclusion_set(exclusion_patterns: &[String]) -> Result<GlobSet> {
  let mut builder = GlobSetBuilder::new();
  for pattern in exclusion_patterns {
    builder.add(pattern::compile(pattern)?);
  }
  builder
    .build()
    .map_err(|e| Error::new(Status::InvalidArg, format!("Failed to build exclusion patterns: {e}")))
}
