//! Embed the exact source revision in generated CI integrations.

use std::{env, path::Path, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    let manifest = env::var("CARGO_MANIFEST_DIR").expect("Cargo provides CARGO_MANIFEST_DIR");
    let revision = Command::new("git")
        .args([
            "-C",
            Path::new(&manifest).to_str().expect("UTF-8 manifest path"),
            "rev-parse",
            "HEAD",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| {
            value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
        })
        .unwrap_or_default();
    println!("cargo:rustc-env=STRUCTTRACE_GIT_SHA={revision}");
}
