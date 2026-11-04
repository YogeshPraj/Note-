//! The key that signs releases, and signing with it. The app holds the public half (`update::UPDATE_KEY`) and installs
//! only what the private half signed.
//!
//! `update_key new <file>`: make a key pair, write the private key to `<file>` and print the public key.
//! `update_key sign <version> <digest file> <signature file>`: sign with the key in `UPDATE_SIGNING_KEY`, then check the
//! result against the key built into the app, so a wrong key stops the release instead of every later update.
#![allow(clippy::print_stdout, clippy::print_stderr, reason = "a command-line tool")]

use std::error::Error;
use std::process::ExitCode;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use notepp_core::update;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

fn new_key(path: &str) -> Result<(), Box<dyn Error>> {
    if std::path::Path::new(path).exists() {
        return Err(format!("{path} exists: a new key would stop every installed copy from updating").into());
    }
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).map_err(|e| e.to_string())?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|e| e.to_string())?;
    std::fs::write(path, STANDARD.encode(pkcs8.as_ref()))?;
    println!("{}", notepp_core::hash::hex(pair.public_key().as_ref()));
    Ok(())
}

fn sign(version: &str, digest_file: &str, signature_file: &str) -> Result<(), Box<dyn Error>> {
    let key = std::env::var("UPDATE_SIGNING_KEY").map_err(|_| "UPDATE_SIGNING_KEY is not set")?;
    let pkcs8 = STANDARD.decode(key.trim()).map_err(|_| "UPDATE_SIGNING_KEY is not a key")?;
    let pair = Ed25519KeyPair::from_pkcs8(&pkcs8).map_err(|_| "UPDATE_SIGNING_KEY is not a key")?;
    let digest = std::fs::read_to_string(digest_file)?;
    let signature = STANDARD.encode(pair.sign(&update::signed_message(version, &digest)));
    update::check_signature(version, &digest, &signature).map_err(|_| "UPDATE_SIGNING_KEY is not the key built into the app")?;
    std::fs::write(signature_file, signature)?;
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["new", path] => new_key(path),
        ["sign", version, digest_file, signature_file] => sign(version, digest_file, signature_file),
        _ => Err("usage: update_key new <file> | update_key sign <version> <digest file> <signature file>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
