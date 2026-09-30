use std::fs::DirEntry;

pub(crate) const UNKNOWN: u8 = 0;
pub(crate) const NO_XMP: u8 = 1;
pub(crate) const MAY_HAVE_XMP: u8 = 2;
const PREFERRED_ONLY: u8 = 3;
const FALLBACK_ONLY: u8 = 4;
const BOTH: u8 = 5;

// Worker-local Bloom filter: only possible matches perform filesystem checks.
// It is consumed before child dispatch, so queued work retains only one byte.
// Saturation creates extra probes, never a false negative or growing memory.
const FILTER_WORDS: usize = 512;
pub(crate) struct DirectoryIndex {
  bits: [u64; FILTER_WORDS],
}

impl DirectoryIndex {
  pub(crate) fn new() -> Self {
    Self {
      bits: [0; FILTER_WORDS],
    }
  }

  pub(crate) fn summarize(&mut self, entries: &[DirEntry], complete: bool) -> u8 {
    if !complete {
      return UNKNOWN;
    }
    let mut found = false;
    for entry in entries {
      let name = entry.file_name();
      let bytes = name.as_encoded_bytes();
      if !is_xmp(bytes) {
        continue;
      }
      // Unicode case folding/normalization is filesystem-dependent. Without
      // a safe ASCII index, preserve literal candidate probing for this parent.
      if !bytes.is_ascii() {
        return MAY_HAVE_XMP;
      }
      if !found {
        self.bits.fill(0);
        found = true;
      }
      self.insert(bytes);
    }
    if found { BOTH } else { NO_XMP }
  }

  pub(crate) fn entry_summary(&self, name: &[u8], summary: u8) -> u8 {
    if summary != BOTH || !name.is_ascii() {
      return if summary == BOTH { MAY_HAVE_XMP } else { summary };
    }
    match (self.contains_candidate(name), self.contains_candidate(stem(name))) {
      (false, false) => NO_XMP,
      (true, false) => PREFERRED_ONLY,
      (false, true) => FALLBACK_ONLY,
      (true, true) => BOTH,
    }
  }

  fn insert(&mut self, name: &[u8]) {
    for bit in positions(name_hash(name.iter().copied())) {
      self.bits[bit / 64] |= 1 << (bit % 64);
    }
  }

  fn contains_candidate(&self, name: &[u8]) -> bool {
    positions(name_hash(name.iter().chain(b".xmp").copied()))
      .into_iter()
      .all(|bit| self.bits[bit / 64] & (1 << (bit % 64)) != 0)
  }
}

fn is_xmp(name: &[u8]) -> bool {
  name.len() >= 4 && name[name.len() - 4..].eq_ignore_ascii_case(b".xmp")
}

fn stem(name: &[u8]) -> &[u8] {
  let end = name.iter().rposition(|&byte| byte == b'.').filter(|&index| index > 0);
  &name[..end.unwrap_or(name.len())]
}

fn name_hash(bytes: impl Iterator<Item = u8>) -> u64 {
  let mut hash = 0xcbf29ce484222325_u64;
  for byte in bytes {
    hash ^= u64::from(byte.to_ascii_lowercase());
    hash = hash.wrapping_mul(0x100000001b3);
  }
  // Mix FNV's bits before selecting three Bloom positions.
  hash ^= hash >> 30;
  hash = hash.wrapping_mul(0xbf58476d1ce4e5b9);
  hash ^= hash >> 27;
  hash = hash.wrapping_mul(0x94d049bb133111eb);
  hash ^ (hash >> 31)
}

fn positions(hash: u64) -> [usize; 3] {
  let mask = FILTER_WORDS * 64 - 1;
  [
    hash as usize & mask,
    (hash >> 21) as usize & mask,
    (hash >> 42) as usize & mask,
  ]
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SidecarResult {
  Found(String),
  Absent,
  Unknown,
}

pub(crate) fn resolve(path: &str, summary: u8) -> SidecarResult {
  #[cfg(unix)]
  {
    resolve_with(path, summary, is_readable)
  }
  #[cfg(not(unix))]
  {
    // Our distributed targets are Unix. Do not claim access compatibility
    // on another platform until an equivalent adapter has been implemented.
    let _ = (path, summary);
    SidecarResult::Unknown
  }
}

fn resolve_with(path: &str, summary: u8, mut readable: impl FnMut(&str) -> bool) -> SidecarResult {
  match summary {
    NO_XMP => return SidecarResult::Absent,
    MAY_HAVE_XMP | PREFERRED_ONLY | FALLBACK_ONLY | BOTH => {}
    _ => return SidecarResult::Unknown,
  }

  if summary != FALLBACK_ONLY {
    let preferred = format!("{path}.xmp");
    if readable(&preferred) {
      return SidecarResult::Found(preferred);
    }
  }
  if summary == PREFERRED_ONLY {
    return SidecarResult::Absent;
  }

  // Match Node's POSIX path.parse: the leading dot of a dotfile is not an
  // extension separator. Remove only the last extension of the basename.
  let basename_start = path.rfind('/').map_or(0, |index| index + 1);
  let basename = &path[basename_start..];
  let stem_end = basename_start + stem(basename.as_bytes()).len();
  let fallback = format!("{}.xmp", &path[..stem_end]);
  if readable(&fallback) {
    SidecarResult::Found(fallback)
  } else {
    SidecarResult::Absent
  }
}

#[cfg(unix)]
fn is_readable(path: &str) -> bool {
  let Ok(path) = std::ffi::CString::new(path) else {
    return false;
  };
  // SAFETY: CString supplies a valid NUL-terminated path for the duration of
  // this call. access(R_OK) follows symlinks and matches Node/libuv on Unix.
  unsafe { libc::access(path.as_ptr(), libc::R_OK) == 0 }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn known_empty_and_unknown_never_probe_candidate_paths() {
    for (summary, expected) in [(NO_XMP, SidecarResult::Absent), (UNKNOWN, SidecarResult::Unknown)] {
      assert_eq!(
        resolve_with("/photos/file.jpg", summary, |_| panic!("unexpected access")),
        expected
      );
    }
  }

  #[test]
  fn incomplete_listing_cannot_establish_absence() {
    let mut index = DirectoryIndex::new();
    assert_eq!(index.summarize(&[], false), UNKNOWN);
    assert_eq!(index.summarize(&[], true), NO_XMP);
  }

  #[test]
  fn indexed_orphans_do_not_cause_negative_probes() {
    let mut index = DirectoryIndex::new();
    index.insert(b"orphan.xmp");
    let summary = index.entry_summary(b"file.jpg", BOTH);
    assert_eq!(summary, NO_XMP);
    assert_eq!(
      resolve_with("/photos/file.jpg", summary, |_| panic!("unexpected access")),
      SidecarResult::Absent
    );
  }

  #[test]
  fn index_keeps_both_candidates_and_shared_stems() {
    let mut index = DirectoryIndex::new();
    index.insert(b"FILE.JPG.XMP");
    index.insert(b"file.xmp");
    assert_eq!(index.entry_summary(b"file.jpg", BOTH), BOTH);
    assert_eq!(index.entry_summary(b"file.raw", BOTH), FALLBACK_ONLY);
    let mut calls = Vec::new();
    assert_eq!(
      resolve_with("/photos/file.raw", FALLBACK_ONLY, |candidate| {
        calls.push(candidate.to_owned());
        true
      }),
      SidecarResult::Found("/photos/file.xmp".into())
    );
    assert_eq!(calls, ["/photos/file.xmp"]);
    calls.clear();
    assert_eq!(
      resolve_with("/photos/file.jpg", PREFERRED_ONLY, |candidate| {
        calls.push(candidate.to_owned());
        false
      }),
      SidecarResult::Absent
    );
    assert_eq!(calls, ["/photos/file.jpg.xmp"]);
  }

  #[test]
  fn saturated_filter_and_non_ascii_names_preserve_candidate_probes() {
    let mut index = DirectoryIndex::new();
    index.bits.fill(u64::MAX);
    assert_eq!(index.entry_summary(b"file.jpg", BOTH), BOTH);
    assert_eq!(index.entry_summary("雪.jpg".as_bytes(), BOTH), MAY_HAVE_XMP);
    assert_eq!(index.entry_summary(b"file.jpg", UNKNOWN), UNKNOWN);
    assert_eq!(index.entry_summary(b"file.jpg", MAY_HAVE_XMP), MAY_HAVE_XMP);
    assert_eq!(std::mem::size_of::<DirectoryIndex>(), 4096);
  }

  #[test]
  fn inserted_names_never_have_false_negatives() {
    let mut index = DirectoryIndex::new();
    for number in 0..100_000 {
      index.insert(format!("{number}.JPG.XMP").as_bytes());
    }
    for number in 0..100_000 {
      let summary = index.entry_summary(format!("{number}.jpg").as_bytes(), BOTH);
      assert!(summary == PREFERRED_ONLY || summary == BOTH);
    }
  }

  #[test]
  fn preferred_sidecar_wins_without_probing_the_fallback() {
    let mut calls = Vec::new();
    let result = resolve_with("/photos/file.jpg", MAY_HAVE_XMP, |candidate| {
      calls.push(candidate.to_owned());
      true
    });
    assert_eq!(result, SidecarResult::Found("/photos/file.jpg.xmp".into()));
    assert_eq!(calls, ["/photos/file.jpg.xmp"]);
  }

  #[test]
  fn failed_preferred_sidecar_falls_back_and_orphans_do_not_match() {
    for available in ["/photos/file.xmp", "/photos/orphan.xmp"] {
      let mut calls = Vec::new();
      let result = resolve_with("/photos/file.jpg", MAY_HAVE_XMP, |candidate| {
        calls.push(candidate.to_owned());
        candidate == available
      });
      assert_eq!(calls, ["/photos/file.jpg.xmp", "/photos/file.xmp"]);
      assert_eq!(
        result,
        if available.ends_with("/file.xmp") {
          SidecarResult::Found(available.into())
        } else {
          SidecarResult::Absent
        }
      );
    }
  }

  #[test]
  fn candidate_stems_match_node_posix_parse() {
    for (path, fallback) in [
      ("/photos/file.edit.jpg", "/photos/file.edit.xmp"),
      ("/photos/.jpg", "/photos/.jpg.xmp"),
      ("/photos/.file.jpg", "/photos/.file.xmp"),
      ("/photos/file.", "/photos/file.xmp"),
      ("/photos/file", "/photos/file.xmp"),
      ("/photos.with.dots/雪.jpg", "/photos.with.dots/雪.xmp"),
    ] {
      let mut calls = Vec::new();
      assert_eq!(
        resolve_with(path, MAY_HAVE_XMP, |candidate| {
          calls.push(candidate.to_owned());
          false
        }),
        SidecarResult::Absent
      );
      assert_eq!(calls, [format!("{path}.xmp"), fallback.to_owned()]);
    }
  }
}
