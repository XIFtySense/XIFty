#![no_main]

use libfuzzer_sys::fuzz_target;
use xifty_container_tiff::parse_bytes;

fuzz_target!(|data: &[u8]| {
    let _ = parse_bytes(data, 0, "fuzz_tiff");
});
