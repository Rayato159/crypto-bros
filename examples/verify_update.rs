//! Read only PUBLIC release artifacts; never accepts or reads a signing private key.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
fn verify(
    bytes: &[u8],
    signature: &str,
    public_key: &str,
    version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let key = PublicKey::decode(&String::from_utf8(STANDARD.decode(public_key.trim())?)?)?;
    let sig = Signature::decode(&String::from_utf8(STANDARD.decode(signature.trim())?)?)?;
    key.verify(bytes, &sig, false)?;
    let signed = sig
        .trusted_comment()
        .split('\t')
        .find_map(|s| s.strip_prefix("version:"));
    if signed != Some(version) {
        return Err("Signed version mismatch".into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("Usage: verify_update <public MSI> <public signature> <version>".into());
    }
    let config: serde_json::Value =
        serde_json::from_str(include_str!("../src-tauri/tauri.conf.json"))?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("Missing public key")?;
    let bytes = std::fs::read(&args[1])?;
    let sig = std::fs::read_to_string(&args[2])?;
    verify(&bytes, &sig, key, &args[3])?;
    let mut damaged = bytes.clone();
    if let Some(b) = damaged.get_mut(64) {
        *b ^= 1;
    }
    if verify(&damaged, &sig, key, &args[3]).is_ok() {
        return Err("Tampered installer accepted".into());
    }
    if verify(&bytes, &sig, key, "999.0.0").is_ok() {
        return Err("Wrong announced version accepted".into());
    }
    println!(
        "Public artifact signature and version verified; changed bytes and wrong version rejected."
    );
    Ok(())
}
