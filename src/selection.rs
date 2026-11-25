use std::collections::BTreeSet;
use std::path::Path;

use napi::bindgen_prelude::*;

use crate::entry_filter::EntryFilter;

pub(crate) const ROOT: &str = "/";

#[derive(Clone, Copy)]
pub(crate) enum Node {
  File,
  Directory,
}

pub(crate) struct Selection {
  pub paths: BTreeSet<String>,
  filter: EntryFilter,
  include_hidden: bool,
}

impl Selection {
  pub fn new(
    paths: &[String],
    include_hidden: Option<bool>,
    exclusion_patterns: Option<Vec<String>>,
    extensions: Option<Vec<String>>,
  ) -> Result<Self> {
    Ok(Self {
      paths: absolute_paths(paths)?,
      filter: EntryFilter::new(&exclusion_patterns.unwrap_or_default(), &extensions.unwrap_or_default())?,
      include_hidden: include_hidden.unwrap_or(false),
    })
  }

  pub fn includes(&self, path: &str) -> bool {
    if !is_normalized(path) {
      return false;
    }

    let mut current = path;
    let mut node = Node::File;
    loop {
      if self.paths.contains(current) && self.includes_root(current, node) {
        return true;
      }
      let Some((parent, name)) = current.rsplit_once('/') else {
        return false;
      };
      if name.is_empty() || !self.includes_name(name.as_bytes()) || !self.includes_child(current, node) {
        return false;
      }
      current = if parent.is_empty() { ROOT } else { parent };
      node = Node::Directory;
    }
  }

  pub fn includes_root(&self, path: &str, node: Node) -> bool {
    match node {
      Node::File => self.filter.includes_file(Path::new(path)),
      Node::Directory => !self.filter.skips_root(Path::new(path)),
    }
  }

  pub fn includes_name(&self, name: &[u8]) -> bool {
    self.include_hidden || !name.starts_with(b".")
  }

  pub fn includes_child(&self, path: &str, node: Node) -> bool {
    match node {
      Node::File => self.filter.includes_file(Path::new(path)),
      Node::Directory => !self.filter.skips_directory(Path::new(path)),
    }
  }
}

fn absolute_paths(paths: &[String]) -> Result<BTreeSet<String>> {
  paths
    .iter()
    .map(|path| match path.starts_with('/') {
      true => Ok(normalize(path)),
      false => Err(Error::new(
        Status::InvalidArg,
        format!("Path must be absolute: '{path}'"),
      )),
    })
    .collect()
}

fn normalize(path: &str) -> String {
  let mut components: Vec<&str> = Vec::new();
  for component in path.split('/') {
    match component {
      "" | "." => {}
      ".." => {
        components.pop();
      }
      component => components.push(component),
    }
  }
  format!("/{}", components.join("/"))
}

fn is_normalized(path: &str) -> bool {
  path
    .strip_prefix('/')
    .is_some_and(|relative| relative.split('/').all(|name| !matches!(name, "" | "." | "..")))
}

#[cfg(test)]
mod tests;
