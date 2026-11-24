use std::borrow::Cow;

use globset::{Glob, GlobBuilder};
use napi::bindgen_prelude::*;

const MAX_BRACE_EXPANSION: usize = 1000;
const RANGE_SEPARATOR: &str = "..";

pub(crate) fn compile(pattern: &str) -> Result<Glob> {
  let prepared = prepare(pattern).map_err(|reason| invalid(pattern, reason))?;
  GlobBuilder::new(&prepared)
    .case_insensitive(true)
    .literal_separator(true)
    .backslash_escape(true)
    .allow_unclosed_class(true)
    .build()
    .map_err(|err| invalid(pattern, &err.to_string()))
}

pub(crate) fn fold_case(text: &str) -> Cow<'_, str> {
  match text.is_ascii() {
    true => Cow::Borrowed(text),
    false => Cow::Owned(text.chars().map(fold_char).collect()),
  }
}

fn fold_char(c: char) -> char {
  if c.is_ascii() || u32::from(c) > u32::from(u16::MAX) {
    return c;
  }

  let mut upper = c.to_uppercase();
  match (upper.next(), upper.next()) {
    (Some(folded), None) if !folded.is_ascii() => folded,
    _ => c,
  }
}

fn invalid(pattern: &str, reason: &str) -> Error {
  Error::new(
    Status::InvalidArg,
    format!("Invalid exclusion pattern '{pattern}': {reason}"),
  )
}

struct Group {
  start: usize,
  listed: bool,
}

fn prepare(pattern: &str) -> std::result::Result<String, &'static str> {
  let mut out = String::with_capacity(pattern.len());
  let mut groups: Vec<Group> = Vec::new();
  let mut expanded: u64 = 0;
  let mut chars = pattern.char_indices();

  while let Some((index, c)) = chars.next() {
    match c {
      '\\' => {
        out.push(c);
        if let Some((_, escaped)) = chars.next() {
          out.push(fold_char(escaped));
        }
      }
      '[' => {
        let class = class_at(&pattern[index..]);
        out.push_str(&fold_case(class));
        for _ in class.chars().skip(1) {
          chars.next();
        }
      }
      '{' => {
        groups.push(Group {
          start: out.len(),
          listed: false,
        });
        out.push(c);
      }
      ',' => {
        if let Some(group) = groups.last_mut() {
          group.listed = true;
        }
        out.push(c);
      }
      '}' => match groups.pop() {
        Some(group) if group.listed => out.push(c),
        Some(group) => {
          let inner = out.split_off(group.start)[1..].to_string();
          match Range::parse(&inner) {
            Some(range) => {
              expanded = expanded.saturating_add(range.len());
              if expanded > MAX_BRACE_EXPANSION as u64 {
                return Err("brace range is too large");
              }
              out.push_str(&format!("{{{}}}", range.items().join(",")));
            }
            None => out.push_str(&format!("\\{{{inner}\\}}")),
          }
        }
        None => out.push_str("\\}"),
      },
      _ => out.push(fold_char(c)),
    }
  }

  for group in groups.iter().rev() {
    out.insert(group.start, '\\');
  }
  Ok(out)
}

fn class_at(text: &str) -> &str {
  let body = &text[1..];
  let skip = usize::from(body.starts_with(['!', '^']));
  let skip = skip + usize::from(body[skip..].starts_with(']'));
  match body[skip..].find(']') {
    Some(end) => &text[..1 + skip + end + 1],
    None => &text[..1],
  }
}

enum Range {
  Number(u64, u64),
  Letter(char, char),
}

impl Range {
  fn parse(inner: &str) -> Option<Self> {
    let (start, end) = inner.split_once(RANGE_SEPARATOR)?;
    if let (Some(start), Some(end)) = (parse_number(start), parse_number(end)) {
      return Some(Self::Number(start.min(end), start.max(end)));
    }
    let (start, end) = (single_letter(start)?, single_letter(end)?);
    Some(Self::Letter(start.min(end), start.max(end)))
  }

  fn len(&self) -> u64 {
    match *self {
      Self::Number(start, end) => (end - start).saturating_add(1),
      Self::Letter(start, end) => u64::from(end) - u64::from(start) + 1,
    }
  }

  fn items(&self) -> Vec<String> {
    match *self {
      Self::Number(start, end) => (start..=end).map(|n| n.to_string()).collect(),
      Self::Letter(start, end) => (start..=end)
        .map(|c| match c.is_ascii_alphanumeric() {
          true => c.to_string(),
          false => format!("\\{c}"),
        })
        .collect(),
    }
  }
}

fn parse_number(text: &str) -> Option<u64> {
  let canonical = text.bytes().all(|b| b.is_ascii_digit()) && (text == "0" || !text.starts_with('0'));
  canonical.then(|| text.parse().ok())?
}

fn single_letter(text: &str) -> Option<char> {
  let mut chars = text.chars();
  match (chars.next(), chars.next()) {
    (Some(c), None) if c.is_ascii_alphabetic() => Some(c),
    _ => None,
  }
}
