#![no_main]

use libfuzzer_sys::fuzz_target;
use structtrace_core::artifact::{PairedCaseRecord, RunManifest};

fuzz_target!(|data: &[u8]| {
    let _ = structtrace_core::strict_json::from_slice::<RunManifest>(data);
    let _ = structtrace_core::strict_json::from_slice::<PairedCaseRecord>(data);
});
