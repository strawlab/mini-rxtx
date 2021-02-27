use serde::{Deserialize, Serialize};

use mini_rxtx::*;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
struct MsgType {
    a: u32,
}

#[cfg(feature = "std")]
#[test]
fn test_roundtrip_std() {
    let msg_orig = MsgType { a: 12345 };

    let buf = serialize_msg_owned(&msg_orig).unwrap();
    let msg_actual = deserialize_owned(&buf).unwrap();
    assert_eq!(msg_orig, msg_actual);
}

#[test]
fn test_roundtrip_zero_size() {
    let mut dest = vec![0u8; 1024];

    let msg_orig = ();
    let buf = serialize_msg(&msg_orig, &mut dest).unwrap();
    let b2 = buf.framed_slice();
    let mut decode_buf = [0u8; 1024];

    println!("() encoded to bytes: {:?}", b2);

    let msg_actual: () = deserialize_owned_borrowed(&b2, &mut decode_buf).unwrap();
    assert_eq!(msg_orig, msg_actual);
}

#[test]
fn test_roundtrip_nostd() {
    let msg_orig = MsgType { a: 12345 };

    let mut dest = vec![0; 1024];
    let encoded = serialize_msg(&msg_orig, &mut dest).unwrap();
    let buf = encoded.framed_slice();

    let mut decode_buf = [0; 1024];
    let msg_actual = deserialize_owned_borrowed(&buf, &mut decode_buf).unwrap(); // requires cargo feature "std"
    assert_eq!(msg_orig, msg_actual);
}
