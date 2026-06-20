# AGENTS.md

Guidance for AI agents (and humans) working in this project.

## What this is

A small **Rust** CLI, **`jw-bible-crossref-graph`**, that turns a JW Library Study Bible
publication (`nwtsty_X.jwpub`) into a **circular cross-reference graphic** — the 66 Bible books
arranged around a ring, with every marginal cross-reference drawn as a chord through the circle,
coloured by source book, and Old↔New Testament links highlighted.

It produces a CSV edge list, an SVG, a 300-dpi PNG, a small cropped README thumbnail, and a
self-contained interactive HTML viewer. It ships as a single static-ish binary (the only native
dependency, SQLite, is bundled and compiled in).

> This was originally a Python/matplotlib script; it was rewritten in Rust so it can ship as
> cross-platform release binaries. Behaviour, flags, output filenames and the layout are preserved.

## Commands

```bash
cargo build --release                          # binary at target/release/jw-bible-crossref-graph
cargo run --release -- nwtsty_X.jwpub          # explicit input
cargo run --release --                         # auto-finds the first *.jwpub in the cwd
cargo run --release -- --lang en               # English labels/UI/CSV headers (default: de)
cargo run --release -- --formats png-sm,html   # produce only some outputs (default: all)

cargo fmt --all --check                        # CI gate
cargo clippy --all-targets -- -D warnings      # CI gate
cargo test                                     # pure-function unit tests (no .jwpub needed)
```

CLI (clap derive in `src/main.rs`):
- `--lang {de,en}` selects the active `STRINGS` entry into `bible.lang` (book names, abbreviations,
  titles, OT/NT labels, HTML UI strings, CSV header). Default `de`.
- `--formats` is a comma-separated subset of `ALL_FORMATS = [db, csv, svg, png, png-sm, html]`, or
  `all` (default). It sets the `Formats` bools. `db` controls whether the extracted `.db` is kept
  (it's always extracted, then deleted unless requested).

Outputs are written **next to the input `.jwpub`**: `nwtsty_X.db`, `bible_crossrefs.csv`,
`bible_circle.svg`, `bible_circle.png`, `bible_circle_sm.png` (small cropped README thumbnail,
re-rendered at decreasing DPI until it fits `cfg::PNG_SM_MAX_BYTES`), `bible_circle.html`.

## The data source (important — no decryption needed)

A `.jwpub` is a ZIP containing `manifest.json` + a `contents` blob. **`contents` is itself a plain
ZIP** (PK header, not encrypted) holding a ~49 MB plaintext **SQLite** database (`nwtsty_X.db`).
`jwpub::extract_db()` does outer-unzip → read `manifest.json` for the db filename
(`publication.fileName`) → inner-unzip → write the `.db`.

Relevant tables (queried in `Bible::load`):

- **`BibleCitation`** — the cross-references = the graph edges. `BibleVerseId` = source verse;
  `FirstBibleVerseId`/`LastBibleVerseId` = target verse range (the code uses the midpoint);
  `MarginalClassification` (0–3) = reference type. Only rows with a non-null source and target are
  used (~65,600 edges; ~7,570 are Old↔New).
- **`BibleBook`** (66 rows) — `FirstVerseId`/`LastVerseId` per book. Books 1–39 = Old Testament,
  40–66 = New Testament.
- **`BibleVerse`** — every verse has a global linear index `BibleVerseId` (0–31193), used as the
  angular coordinate on the ring.
- **`BibleChapter`** (`BookNumber`, `ChapterNumber`, `FirstVerseId`, `LastVerseId`) — used to turn
  a verse id back into a label like `Jes 7:14`.

This particular publication is the **German** Study Bible (Studienbibel). The `.jwpub` supplies
only verse *ids*; book names and all UI text come from the `STRINGS` table, so `--lang` only
relabels the graphic (default German) — it does not translate any Bible text.

## Architecture / module layout

`main.rs` runs a linear pipeline (`extract → load → csv → static(svg/png) → png-sm → html`):

- **`src/jwpub.rs`** — `extract_db()` (zip-in-zip → SQLite file) via the `zip` + `serde_json` crates.
- **`src/bible.rs`** — the `Bible` struct and `Bible::load()` (3 `rusqlite` SELECTs), plus
  `layout_books()` (assigns each book an angular wedge), `book_idx_of` (`partition_point` =
  Python's `bisect_right`), `verse_angle`, `verse_label`, `verse_xy`. Also the `cfg` module of
  geometry/style consts, `chord_control`, `hsv_to_rgb`/`book_hue_rgb`/`hex`, and `#[cfg(test)]`
  unit tests.
- **`src/render.rs`** — the renderers. `build_svg(&Bible, draw_text) -> String` is the shared
  renderer (replaces the old matplotlib `build_figure`): faint all-edges layer + highlighted
  Old↔New layer + a coloured rim arc per book, plus labels/title when `draw_text`. `svg_to_pixmap`
  rasterizes via **resvg/usvg/tiny-skia**; `crop_nonwhite` trims the white border for the
  thumbnail. `render_static` (SVG+PNG), `render_png_sm` (DPI-ladder byte budget), `render_html`
  (label-free PNG background base64-embedded + interactive Old↔New `<path>`s with `<title>`
  tooltips + vector labels + vanilla JS). Text is rendered with a host system font picked by
  `text_options()`/`pick_sans_family()` (no font is bundled).
- **`src/strings.rs`** — `enum Lang`, the `Strings` struct (`#[derive(Deserialize)]`), and a
  `LazyLock` per language that parses the embedded YAML (`include_str!("../i18n/<lang>.yaml")`) via
  `serde_yaml_ng`, validating 66 `abbr`/`name` entries + 6 `csv_header` columns. `N_OT_BOOKS = 39`.
- **`i18n/<lang>.yaml`** — the actual translations (titles, OT/NT, HTML UI strings, `csv_header`,
  and the 66-book `abbr`/`name` lists). Edited without touching Rust; embedded at build time.

## Layout logic (where the visual structure lives)

- The Old Testament fills the **left half** `[90°+GAP .. 270°−GAP]`; the New Testament fills the
  **right half** `[270°+GAP .. 450°−GAP]`. The two seam gaps land at top (Offenbarung↔1.Mose) and
  bottom (Maleachi↔Matthäus).
- Within each half, every book gets a wedge **proportional to its verse count**, separated by
  `BOOK_GAP_DEG`. The NT adds `BRACKET_GAP_DEG` before Matthäus and after Johannes to bracket the
  four Gospels.
- Chords are quadratic Béziers with the control point pulled toward the centre by `BOW`.

## Tuning

Geometry/style live in the `cfg` module in `src/bible.rs`: `R`, `L` (canvas half-extent — increase
if long labels clip), `GAP_DEG`, `BOOK_GAP_DEG`, `BRACKET_GAP_DEG`, `BOW`; `FAINT_ALPHA`/`FAINT_LW`,
`HL_ALPHA`/`HL_LW`, `RIM_LW`, `LABEL_FONTSIZE`; `FIG_INCHES`, `DPI`; and the thumbnail knobs
`PNG_SM_MAX_BYTES` / `PNG_SM_DPIS`. Stroke widths are matplotlib point sizes converted to world
units via `PT2WORLD` (the consts in `render.rs`) — this is the main lever if chord density looks off.

Localisation lives in `i18n/<lang>.yaml` (parsed by `src/strings.rs`).

## Gotchas

- **Coordinate transforms:** world space is y-up; the SVG is y-down, so `render.rs` negates y on
  output. Rim arcs use the SVG `A` command with `large-arc=0`, `sweep=1` — if a rim bows the wrong
  way, that's the flag to flip.
- **resvg/usvg/tiny-skia/fontdb must stay a matched set** (resvg re-exports its own usvg/tiny-skia).
  Bump them together; `dependabot.yml` groups them.
- **Fonts come from the host system** (`fontdb::load_system_fonts`); no font is bundled.
  `pick_sans_family` chooses the first installed of Arial/Helvetica/Liberation Sans/DejaVu Sans/…
  Renders are therefore *not* byte-reproducible across machines, and a fontless host yields a
  graphic with no text labels (chords/arcs still render). The `<text>` carries no `font-family`,
  so usvg falls back to `Options.font_family`.
- The SVG/PNG contain ~65k chord paths, so the in-memory SVG is ~9–16 MB and rendering takes a few
  seconds; that's expected. The HTML keeps only the ~7.5k Old↔New chords interactive (the
  background is a raster).
- **Releases**: `release-please` (config in `.github/`) drives versioning from Conventional
  Commits; the `Release` workflow builds the 7 platform binaries with `cross` and uploads them.
  `Cargo.lock` is committed (this is a binary).
