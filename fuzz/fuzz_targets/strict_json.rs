#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = structtrace_core::strict_json::value_from_slice(data);
    for line in data.split(|byte| *byte == b'\n') {
        let _ = structtrace_core::strict_json::value_from_slice(line);
    }
});
