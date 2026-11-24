use std::path::Path;

use globset::{GlobSet, GlobSetBuilder};
use napi::bindgen_prelude::*;

use crate::extension_filter::ExtensionFilter;

pub(crate) struct EntryFilter {
  exclusion_set: GlobSet,
  directory_exclusion_set: GlobSet,
  extension_filter: ExtensionFilter,
}

impl EntryFilter {
  pub fn new(exclusion_patterns: &[String], extensions: &[String]) -> Result<Self> {
    let directory_patterns: Vec<String> = exclusion_patterns
      .iter()
      .filter_map(|pattern| pattern.strip_suffix("/**").map(String::from))
      .collect();
    Ok(Self {
      exclusion_set: build_exclusion_set(exclusion_patterns)?,
      directory_exclusion_set: build_exclusion_set(&directory_patterns)?,
      extension_filter: ExtensionFilter::new(extensions),
    })
  }

  pub fn skips_root(&self, path: &Path) -> bool {
    self.exclusion_set.is_match(path)
  }

  pub fn skips_directory(&self, path: &Path) -> bool {
    self.directory_exclusion_set.is_match(path) || self.skips_root(path)
  }

  pub fn includes_file(&self, path: &Path) -> bool {
    !self.exclusion_set.is_match(path) && self.extension_filter.is_match(path)
  }
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
