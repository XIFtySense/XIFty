#![no_main]

use libfuzzer_sys::fuzz_target;
use xifty_container_jpeg::parse_bytes;

fuzz_target!(|data: &[u8]| {
    let _ = parse_bytes(data, 0);
});
