use std::path::Path;

pub(crate) struct ExtensionFilter(Vec<String>);

impl ExtensionFilter {
  pub fn new(extensions: &[String]) -> Self {
    Self(
      extensions
        .iter()
        .map(|ext| ext.strip_prefix('.').unwrap_or(ext).to_lowercase())
        .collect(),
    )
  }

  pub fn is_match(&self, path: &Path) -> bool {
    self.0.is_empty()
      || path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| self.0.iter().any(|e| e.eq_ignore_ascii_case(ext)))
  }
}
