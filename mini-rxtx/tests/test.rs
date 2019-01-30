#[macro_use]
extern crate serde_derive;
extern crate serde;

use mini_rxtx::*;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
struct MsgType {
    a: u32,
}

#[test]
fn test_roundtrip() {
    let msg_orig = MsgType{
        a: 12345,
    };

    let buf = serialize_msg_owned(&msg_orig).unwrap();
    let msg_actual = deserialize_owned(&buf).unwrap();
    assert_eq!(msg_orig, msg_actual);
}
