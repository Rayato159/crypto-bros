use std::{env, process::Command};

fn main() {
    for path in [
        "src/live-wallet.js",
        "scripts/bundle-wallet.mjs",
        "package-lock.json",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    let output = env::var("OUT_DIR").expect("Cargo supplies OUT_DIR");
    let status = Command::new("node")
        .args(["scripts/bundle-wallet.mjs", &output])
        .status()
        .expect("Node is required to bundle the Phantom approval page; run npm ci first");
    assert!(
        status.success(),
        "Wallet page bundling failed; run npm ci first"
    );
}
