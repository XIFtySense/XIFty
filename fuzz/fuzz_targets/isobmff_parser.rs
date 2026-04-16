#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = xifty_container_isobmff::parse_bytes(data, 0);
});
