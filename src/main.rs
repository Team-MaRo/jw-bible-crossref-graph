//! Build a circular cross-reference graphic from a JW Library Study Bible
//! (.jwpub). Re-run after a Bible update to regenerate everything.

mod bible;
mod jwpub;
mod render;
mod strings;

use anyhow::{bail, Context, Result};
use bible::Bible;
use clap::Parser;
use std::path::{Path, PathBuf};
use strings::Lang;

const ALL_FORMATS: [&str; 6] = ["db", "csv", "svg", "png", "png-sm", "html"];

#[derive(Parser)]
#[command(
    name = "jw-bible-crossref-graph",
    version,
    about = "Build a circular cross-reference graphic from a JW Library Study Bible (.jwpub)."
)]
struct Args {
    /// Path to the .jwpub (default: first *.jwpub in the current directory).
    jwpub: Option<PathBuf>,

    /// Language for book names / labels / UI text.
    #[arg(long, value_enum, default_value = "de")]
    lang: Lang,

    /// Comma-separated outputs to produce, or 'all'.
    /// Choices: db, csv, svg, png, png-sm, html.
    #[arg(long, default_value = "all")]
    formats: String,
}

struct Formats {
    db: bool,
    csv: bool,
    svg: bool,
    png: bool,
    png_sm: bool,
    html: bool,
}

fn resolve_formats(spec: &str) -> Result<Formats> {
    let chosen: Vec<String> = if spec.trim().eq_ignore_ascii_case("all") {
        ALL_FORMATS.iter().map(|s| s.to_string()).collect()
    } else {
        spec.split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    };
    if chosen.is_empty() {
        bail!(
            "No formats selected. Use --formats all or a comma-separated subset of: {}",
            ALL_FORMATS.join(", ")
        );
    }
    let invalid: Vec<&str> = chosen
        .iter()
        .filter(|f| !ALL_FORMATS.contains(&f.as_str()))
        .map(|s| s.as_str())
        .collect();
    if !invalid.is_empty() {
        bail!(
            "Unknown format(s): {}\nValid choices: {}, or 'all'.",
            invalid.join(", "),
            ALL_FORMATS.join(", ")
        );
    }
    let has = |k: &str| chosen.iter().any(|c| c == k);
    Ok(Formats {
        db: has("db"),
        csv: has("csv"),
        svg: has("svg"),
        png: has("png"),
        png_sm: has("png-sm"),
        html: has("html"),
    })
}

fn first_jwpub(dir: &Path) -> Option<PathBuf> {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jwpub"))
        })
        .collect();
    hits.sort();
    hits.into_iter().next()
}

fn find_jwpub(arg: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = arg {
        return Ok(p);
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(p) = first_jwpub(&cwd) {
            return Ok(p);
        }
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        if let Some(p) = first_jwpub(&dir) {
            return Ok(p);
        }
    }
    bail!("No .jwpub given and none found in this folder.\nUsage: jw-bible-crossref-graph nwtsty_X.jwpub");
}

fn run() -> Result<()> {
    let args = Args::parse();
    let fmts = resolve_formats(&args.formats)?;

    let jwpub = std::fs::canonicalize(find_jwpub(args.jwpub)?).context("resolving .jwpub path")?;
    let out_dir = jwpub.parent().unwrap_or(Path::new(".")).to_path_buf();

    let selected: Vec<&str> = [
        ("db", fmts.db),
        ("csv", fmts.csv),
        ("svg", fmts.svg),
        ("png", fmts.png),
        ("png-sm", fmts.png_sm),
        ("html", fmts.html),
    ]
    .into_iter()
    .filter_map(|(k, on)| on.then_some(k))
    .collect();

    println!("Input : {}", jwpub.display());
    println!(
        "Lang  : {}    Formats: {}",
        args.lang.code(),
        selected.join(", ")
    );
    println!("Output: {}\n", out_dir.display());

    let db_path = jwpub::extract_db(&jwpub, &out_dir)?;
    let bible = Bible::load(&db_path, args.lang)?;

    if fmts.csv {
        render::write_csv(&bible, &out_dir.join("bible_crossrefs.csv"))?;
    }

    let svg_path = out_dir.join("bible_circle.svg");
    let png_path = out_dir.join("bible_circle.png");
    if fmts.svg || fmts.png {
        render::render_static(&bible, &svg_path, &png_path, fmts.svg, fmts.png)?;
    }
    if fmts.png_sm {
        render::render_png_sm(&bible, &out_dir.join("bible_circle_sm.png"))?;
    }
    if fmts.html {
        render::render_html(&bible, &out_dir.join("bible_circle.html"))?;
    }

    if !fmts.db {
        let _ = std::fs::remove_file(&db_path);
    }

    println!("\n[7/7] Done. Files written to: {}", out_dir.display());
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
