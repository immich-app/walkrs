use std::path::Path;

pub(crate) struct ExtensionFilter(Vec<String>);

impl ExtensionFilter {
  pub fn new(extensions: &[String]) -> Self {
    let mut extensions: Vec<String> = extensions
      .iter()
      .map(|ext| ext.strip_prefix('.').unwrap_or(ext).to_lowercase())
      .collect();
    extensions.sort();
    extensions.dedup();
    Self(extensions)
  }

  pub fn is_match(&self, path: &Path) -> bool {
    self.0.is_empty()
      || path.extension().and_then(|e| e.to_str()).is_some_and(|ext| {
        let mut buf = [0u8; 16];
        let ext = ext.as_bytes();
        if ext.len() > buf.len() {
          return false;
        }
        let slot = &mut buf[..ext.len()];
        slot.copy_from_slice(ext);
        slot.make_ascii_lowercase();
        let lowered = unsafe { std::str::from_utf8_unchecked(slot) };
        self.0.binary_search_by_key(&lowered, |s| s.as_str()).is_ok()
      })
  }
}
