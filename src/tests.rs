use super::*;
use std::time::Duration;

#[test]
fn modification_times_are_signed_integer_milliseconds() {
  assert_eq!(timestamp_millis(UNIX_EPOCH), Ok(0));
  assert_eq!(
    timestamp_millis(UNIX_EPOCH + Duration::from_micros(1_234_567)),
    Ok(1234)
  );
  assert_eq!(
    timestamp_millis(UNIX_EPOCH - Duration::from_micros(1_234_567)),
    Ok(-1234)
  );
  assert_eq!(timestamp_millis(UNIX_EPOCH - Duration::from_micros(999)), Ok(0));
}

#[test]
fn modification_times_outside_the_safe_integer_range_are_errors() {
  assert!(timestamp_millis(UNIX_EPOCH + Duration::from_millis(MAX_SAFE_INTEGER + 1)).is_err());
  assert!(timestamp_millis(UNIX_EPOCH - Duration::from_millis(MAX_SAFE_INTEGER + 1)).is_err());
}
