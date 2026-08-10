#![no_main]

use libfuzzer_sys::fuzz_target;
use structtrace_adapters::protocol::VariantResponse;

fuzz_target!(|data: &[u8]| {
    let _ = structtrace_core::strict_json::from_slice::<VariantResponse>(data);
});
