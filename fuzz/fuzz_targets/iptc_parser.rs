#![no_main]

use libfuzzer_sys::fuzz_target;
use xifty_meta_iptc::{IptcPayload, decode_payload};

fuzz_target!(|data: &[u8]| {
    let _ = decode_payload(IptcPayload {
        bytes: data,
        container: "fuzz",
        path: "iptc",
        offset_start: 0,
        offset_end: data.len() as u64,
    });
});
