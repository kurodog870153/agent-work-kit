use super::*;

#[test]
fn pointers_round_trip_and_reject_bad_escapes() {
    assert_eq!(encode_segment("a/b~c"), "a~1b~0c");
    assert_eq!(decode_pointer("/a~1b~0c").unwrap(), vec!["a/b~c"]);
    assert!(decode_pointer("/").is_err());
    assert!(decode_pointer("a").is_err());
    assert!(decode_pointer("/bad~2name").is_err());
}
