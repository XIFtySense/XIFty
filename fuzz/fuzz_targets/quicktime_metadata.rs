#![no_main]

use libfuzzer_sys::fuzz_target;
use xifty_meta_quicktime::{QuickTimePayload, decode_payload};

fuzz_target!(|data: &[u8]| {
    for key in ["author", "software", "title", "unknown"] {
        let _ = decode_payload(QuickTimePayload {
            key,
            bytes: data,
            container: "mp4",
            offset_start: 0,
            offset_end: data.len() as u64,
        });
    }
});
