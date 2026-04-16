#![no_main]

use libfuzzer_sys::fuzz_target;
use xifty_meta_xmp::{XmpPacket, decode_packet};

fuzz_target!(|data: &[u8]| {
    let _ = decode_packet(XmpPacket {
        bytes: data,
        container: "fuzz",
        offset_start: 0,
        offset_end: data.len() as u64,
    });
});
