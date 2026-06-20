//! Extract the SQLite DB from a `.jwpub` (a zip whose `contents` member is
//! itself a plain zip holding the plaintext database). No decryption needed.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Manifest {
    publication: Publication,
}

#[derive(Deserialize)]
struct Publication {
    #[serde(rename = "fileName")]
    file_name: String,
}

pub fn extract_db(jwpub: &Path, out_dir: &Path) -> Result<PathBuf> {
    let name = jwpub.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    println!("[1/7] Opening {name} ...");

    let file =
        std::fs::File::open(jwpub).with_context(|| format!("opening {}", jwpub.display()))?;
    let mut outer = zip::ZipArchive::new(file).context("reading .jwpub (outer zip)")?;

    let manifest: Manifest = {
        let mut entry = outer
            .by_name("manifest.json")
            .context("manifest.json not found")?;
        let mut json = String::new();
        entry.read_to_string(&mut json)?;
        serde_json::from_str(&json).context("parsing manifest.json")?
    };
    let db_name = manifest.publication.file_name;
    println!("      manifest -> publication db: {db_name}");

    let mut contents = Vec::new();
    outer
        .by_name("contents")
        .context("'contents' blob not found")?
        .read_to_end(&mut contents)?;

    let mut inner = zip::ZipArchive::new(Cursor::new(contents)).context("reading inner zip")?;
    let mut db_bytes = Vec::new();
    inner
        .by_name(&db_name)
        .with_context(|| format!("{db_name} not found in inner zip"))?
        .read_to_end(&mut db_bytes)?;

    let db_path = out_dir.join(&db_name);
    std::fs::write(&db_path, &db_bytes)
        .with_context(|| format!("writing {}", db_path.display()))?;
    println!(
        "      extracted DB -> {}  ({:.1} MB)",
        db_path.display(),
        db_bytes.len() as f64 / 1e6
    );
    Ok(db_path)
}
