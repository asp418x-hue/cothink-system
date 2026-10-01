//! Downloads the LiteRT-LM native libraries (the .dylib/.so the FFI needs) from
//! this project's own GitHub release into `./native/<platform>/`.
//!
//! Usage:
//!   cargo run --bin fetch_natives
//!
//! The binaries are hosted on our own SHA256-verified release, so we don't
//! depend on any third party.
use std::fs;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

const TAG: &str = "native-v0.13.1";
const BASE: &str = "https://github.com/JhonaCodes/litert-rs/releases/download";

fn platform() -> (&'static str, &'static str) {
    // (dirName, sha256 of the tar.gz)
    if cfg!(target_os = "macos") {
        (
            "macos_arm64",
            "59d7f73d1cb4077ad82fefb0a8d2a506d8c89d61487a7981816dbffe8a3cbe2d",
        )
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        (
            "linux_arm64",
            "79ec60b99076e53bd51e1be4bedeb45abb8d724d1bb45c8acc5c171cc4cde7bd",
        )
    } else {
        (
            "linux_x86_64",
            "2c84bdacd4f367a631270a6b3f8ff87e2d11aa019d8e898a2361f40591667024",
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (dir, sha) = platform();
    let archive = format!("litertlm-{dir}.tar.gz");
    let url = format!("{BASE}/{TAG}/{archive}");
    let dest = Path::new("native").join(dir);

    println!("Downloading {archive} ({TAG})…");
    let mut bytes = Vec::new();
    ureq::get(&url)
        .call()?
        .into_reader()
        .read_to_end(&mut bytes)?;

    println!("Verifying SHA256…");
    let got = format!("{:x}", Sha256::digest(&bytes));
    if got != sha {
        return Err(format!("SHA256 mismatch:\n  expected: {sha}\n  got:      {got}").into());
    }

    println!("Extracting to {}…", dest.display());
    fs::create_dir_all(&dest)?;
    let mut copied = 0usize;
    let mut tar_archive = Archive::new(GzDecoder::new(&bytes[..]));
    for entry in tar_archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let is_lib = name.ends_with(".dylib") || name.ends_with(".so") || name.contains(".so.");
        if !is_lib {
            continue;
        }
        entry.unpack(dest.join(&name))?;
        copied += 1;
    }

    if copied == 0 {
        return Err(format!("no libraries found inside {archive}").into());
    }
    println!("Done: {copied} libraries in {}", dest.display());
    Ok(())
}
