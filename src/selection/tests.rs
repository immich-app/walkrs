use super::*;

#[test]
fn normalizes_paths_lexically() {
  let cases = [
    ("/lib", "/lib"),
    ("/lib/", "/lib"),
    ("/lib//", "/lib"),
    ("//lib/./sub/../", "/lib"),
    ("/lib/.", "/lib"),
    ("/..", "/"),
    ("/", "/"),
  ];

  for (path, expected) in cases {
    assert_eq!(normalize(path), expected, "{path}");
  }
}
