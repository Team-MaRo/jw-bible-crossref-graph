//! Load books, chapters and cross-reference edges; lay each book out as an
//! angular wedge on the ring; map verse ids to angles and labels.

use crate::strings::{Lang, N_OT_BOOKS};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

/// Tunable geometry / style knobs (port of the Python `CFG` dict).
pub mod cfg {
    pub const R: f64 = 1.0; // ring radius
    pub const L: f64 = 1.78; // canvas half-extent (room for long labels)
    pub const GAP_DEG: f64 = 7.0; // testament seams (top/bottom)
    pub const BOOK_GAP_DEG: f64 = 0.7; // gap between adjacent books
    pub const BRACKET_GAP_DEG: f64 = 4.5; // extra gap bracketing the Gospels
    pub const BOW: f64 = 0.32; // chord control-point pull toward centre

    pub const FAINT_ALPHA: f64 = 0.06;
    pub const FAINT_LW: f64 = 0.20; // points (matplotlib units)
    pub const HL_ALPHA: f64 = 0.55;
    pub const HL_LW: f64 = 0.55;
    pub const RIM_LW: f64 = 3.0;
    pub const LABEL_FONTSIZE: f64 = 5.0;

    pub const FIG_INCHES: f64 = 13.0;
    pub const DPI: f64 = 300.0;

    /// Convert a matplotlib point size to world units: the 13in figure spans
    /// `2L` world units, and 1pt = 1/72in.
    pub const PT2WORLD: f64 = (2.0 * L) / (72.0 * FIG_INCHES);

    pub const PNG_SM_MAX_BYTES: usize = 500_000;
    pub const PNG_SM_DPIS: [f64; 10] =
        [130.0, 110.0, 92.0, 80.0, 72.0, 64.0, 60.0, 56.0, 52.0, 48.0];
}

pub struct Book {
    pub idx: usize,
    pub abbr: &'static str,
    pub name: &'static str,
    pub first: i64,
    pub last: i64,
    pub is_ot: bool,
    pub hue: f64,
    pub a0: f64, // wedge start (degrees, CCW from +x)
    pub a1: f64, // wedge end
    pub mid: f64,
}

pub struct Edge {
    pub src: i64,
    pub tgt: i64,
    pub sb: usize, // source book index
    pub tb: usize, // target book index
    pub cls: i64,
    pub cross: bool, // links OT <-> NT
}

pub struct Bible {
    pub books: Vec<Book>,
    book_first: Vec<i64>,
    ch_first: Vec<i64>,
    chapters: Vec<(i64, i64, i64, i64)>, // book, chapter, first, last
    pub edges: Vec<Edge>,
    pub lang: Lang,
}

impl Bible {
    pub fn load(db: &Path, lang: Lang) -> Result<Bible> {
        println!("[2/7] Reading Bible structure and cross-references ...");
        let s = lang.strings();
        let conn = Connection::open(db).context("opening SQLite db")?;

        // --- books ---
        let book_rows: Vec<(i64, i64, i64)> = conn
            .prepare(
                "SELECT BibleBookId, FirstVerseId, LastVerseId FROM BibleBook ORDER BY BibleBookId",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let n = book_rows.len();
        let mut books = Vec::with_capacity(n);
        let mut book_first = Vec::with_capacity(n);
        for (bid, first, last) in book_rows {
            let idx = (bid - 1) as usize;
            let abbr = s.abbr.get(idx).map_or("?", String::as_str);
            let name = s.name.get(idx).map_or(abbr, String::as_str);
            books.push(Book {
                idx,
                abbr,
                name,
                first,
                last,
                is_ot: bid <= N_OT_BOOKS,
                hue: idx as f64 / n as f64,
                a0: 0.0,
                a1: 0.0,
                mid: 0.0,
            });
            book_first.push(first);
        }
        let v_min = books[0].first;
        let v_max = books[n - 1].last;
        let nt_start = books[N_OT_BOOKS as usize].first;
        layout_books(&mut books);

        // --- chapters (for human-readable verse labels) ---
        let chapters: Vec<(i64, i64, i64, i64)> = conn
            .prepare("SELECT BookNumber, ChapterNumber, FirstVerseId, LastVerseId FROM BibleChapter ORDER BY FirstVerseId")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let ch_first = chapters.iter().map(|c| c.2).collect();

        let mut bible = Bible {
            books,
            book_first,
            ch_first,
            chapters,
            edges: Vec::new(),
            lang,
        };

        // --- cross-reference edges ---
        let cites: Vec<(i64, i64, Option<i64>, Option<i64>)> = conn
            .prepare(
                "SELECT BibleVerseId, FirstBibleVerseId, LastBibleVerseId, MarginalClassification \
                 FROM BibleCitation WHERE BibleVerseId IS NOT NULL AND FirstBibleVerseId IS NOT NULL",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;

        for (src, tfirst, tlast, cls) in cites {
            let tgt = (tfirst + tlast.unwrap_or(tfirst)) / 2;
            let (Some(sb), Some(tb)) = (bible.book_idx_of(src), bible.book_idx_of(tgt)) else {
                continue;
            };
            let cross = (src < nt_start) != (tgt < nt_start);
            bible.edges.push(Edge {
                src,
                tgt,
                sb,
                tb,
                cls: cls.unwrap_or(0),
                cross,
            });
        }

        let n_cross = bible.edges.iter().filter(|e| e.cross).count();
        println!("      books: {}   verses: {}..{}", n, v_min, v_max);
        println!(
            "      edges: {}   Old<->New Testament: {}",
            bible.edges.len(),
            n_cross
        );
        Ok(bible)
    }

    /// verse id -> book index (0..65), or None if outside any book.
    pub fn book_idx_of(&self, vid: i64) -> Option<usize> {
        let i = self.book_first.partition_point(|&x| x <= vid); // == bisect_right
        if i == 0 {
            return None;
        }
        let b = &self.books[i - 1];
        (b.first <= vid && vid <= b.last).then_some(i - 1)
    }

    /// verse id -> "Abbr C:V".
    pub fn verse_label(&self, vid: i64) -> String {
        let abbr = self.book_idx_of(vid).map_or("?", |bi| self.books[bi].abbr);
        let ci = self.ch_first.partition_point(|&x| x <= vid);
        if ci >= 1 {
            let (_, chap, cfirst, clast) = self.chapters[ci - 1];
            if cfirst <= vid && vid <= clast {
                return format!("{abbr} {chap}:{}", vid - cfirst + 1);
            }
        }
        abbr.to_string()
    }

    /// verse id -> angle on the ring (radians, math convention).
    pub fn verse_angle(&self, vid: i64) -> f64 {
        let b = self
            .book_idx_of(vid)
            .map_or(&self.books[0], |i| &self.books[i]);
        let span = (b.last - b.first).max(1) as f64;
        let frac = (((vid - b.first) as f64) / span).clamp(0.0, 1.0);
        (b.a0 + frac * (b.a1 - b.a0)).to_radians()
    }

    pub fn verse_xy(&self, vid: i64, r: f64) -> (f64, f64) {
        let a = self.verse_angle(vid);
        (r * a.cos(), r * a.sin())
    }
}

/// Lay each book out as an angular wedge. OT fills the left half
/// `[90+seam, 270-seam]`, NT the right half `[270+seam, 450-seam]`, each wedge
/// proportional to the book's verse count, with gaps between books and an extra
/// bracket around the four Gospels.
fn layout_books(books: &mut [Book]) {
    let seam = cfg::GAP_DEG;
    let bgap = cfg::BOOK_GAP_DEG;
    let bracket = cfg::BRACKET_GAP_DEG;

    let ot: Vec<usize> = (0..books.len()).filter(|&i| books[i].is_ot).collect();
    let nt: Vec<usize> = (0..books.len()).filter(|&i| !books[i].is_ot).collect();

    let mut ot_gaps = vec![bgap; ot.len()];
    if !ot_gaps.is_empty() {
        ot_gaps[0] = 0.0;
    }
    lay(books, &ot, 90.0 + seam, 270.0 - seam, &ot_gaps);

    let mut nt_gaps = vec![bgap; nt.len()];
    if !nt_gaps.is_empty() {
        nt_gaps[0] = bracket; // before Matthäus
    }
    if nt.len() > 4 {
        nt_gaps[4] = bgap + bracket; // before Apostelgeschichte (after Johannes)
    }
    lay(books, &nt, 270.0 + seam, 450.0 - seam, &nt_gaps);
}

fn lay(books: &mut [Book], idxs: &[usize], start: f64, end: f64, gap_before: &[f64]) {
    let total_span = end - start;
    let total_gap: f64 = gap_before.iter().sum();
    let usable = total_span - total_gap;
    let verses = |b: &Book| (b.last - b.first + 1).max(1) as f64;
    let total_verses: f64 = idxs.iter().map(|&i| verses(&books[i])).sum();
    let mut cursor = start;
    for (k, &i) in idxs.iter().enumerate() {
        cursor += gap_before[k];
        let w = usable * verses(&books[i]) / total_verses;
        books[i].a0 = cursor;
        books[i].a1 = cursor + w;
        books[i].mid = cursor + w / 2.0;
        cursor = books[i].a1;
    }
}

/// Quadratic Bezier control point: midpoint pulled toward the centre.
pub fn chord_control(x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64) {
    ((x1 + x2) * 0.5 * cfg::BOW, (y1 + y2) * 0.5 * cfg::BOW)
}

/// HSV -> RGB, matching Python's `colorsys.hsv_to_rgb`.
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (v, v, v);
    }
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match (i as i64).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// A book's rim/chord colour (rainbow by position in the canon).
pub fn book_hue_rgb(hue: f64) -> (f64, f64, f64) {
    hsv_to_rgb(hue, 0.80, 0.85)
}

/// "#rrggbb" from float rgb in 0..1 (truncating, like Python `int(x*255)`).
pub fn hex(rgb: (f64, f64, f64)) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgb.0 * 255.0) as u8,
        (rgb.1 * 255.0) as u8,
        (rgb.2 * 255.0) as u8
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_matches_colorsys() {
        // colorsys.hsv_to_rgb(0, .8, .85) -> (0.85, 0.17, 0.17)
        let (r, g, b) = hsv_to_rgb(0.0, 0.80, 0.85);
        assert!((r - 0.85).abs() < 1e-9);
        assert!((g - 0.17).abs() < 1e-9);
        assert!((b - 0.17).abs() < 1e-9);
        // achromatic
        assert_eq!(hsv_to_rgb(0.3, 0.0, 0.5), (0.5, 0.5, 0.5));
    }

    #[test]
    fn hex_truncates() {
        assert_eq!(hex((1.0, 0.0, 0.0)), "#ff0000");
        // truncating like Python int(): 0.85*255=216.75 -> 216 (0xd8)
        assert_eq!(hex((0.85, 0.17, 0.17)), "#d82b2b");
    }

    #[test]
    fn layout_is_monotonic_and_within_seams() {
        // synthetic 66 books, 100 verses each
        let mut books: Vec<Book> = (0..66)
            .map(|i| Book {
                idx: i,
                abbr: "x",
                name: "x",
                first: (i as i64) * 100,
                last: (i as i64) * 100 + 99,
                is_ot: (i as i64) < N_OT_BOOKS,
                hue: 0.0,
                a0: 0.0,
                a1: 0.0,
                mid: 0.0,
            })
            .collect();
        layout_books(&mut books);

        // OT wedges live within [90+seam, 270-seam], strictly increasing, non-overlapping
        let seam = cfg::GAP_DEG;
        let mut prev = 90.0 + seam - 1e-9;
        for b in books.iter().filter(|b| b.is_ot) {
            assert!(b.a0 >= prev - 1e-6, "OT wedge overlaps predecessor");
            assert!(b.a1 > b.a0, "wedge has positive width");
            assert!(b.a1 <= 270.0 - seam + 1e-6, "OT exceeds bottom seam");
            prev = b.a1;
        }
        // NT within [270+seam, 450-seam]
        let mut prev = 270.0 + seam - 1e-9;
        for b in books.iter().filter(|b| !b.is_ot) {
            assert!(b.a0 >= prev - 1e-6);
            assert!(b.a1 <= 450.0 - seam + 1e-6, "NT exceeds top seam");
            prev = b.a1;
        }
    }
}
