#![no_main]

use libfuzzer_sys::fuzz_target;
use xifty_meta_icc::{IccPayload, decode_payload};

fuzz_target!(|data: &[u8]| {
    let _ = decode_payload(IccPayload {
        bytes: data,
        container: "fuzz",
        path: "icc",
        offset_start: 0,
        offset_end: data.len() as u64,
    });
});
