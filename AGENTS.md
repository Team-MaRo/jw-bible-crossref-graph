# AGENTS.md

Guidance for AI agents (and humans) working in this project.

## What this is

A single self-contained Python script, **`bible_circle.py`**, that turns a JW Library
Study Bible publication (`nwtsty_X.jwpub`) into a **circular cross-reference graphic** —
the 66 Bible books arranged around a ring, with every marginal cross-reference drawn as a
chord through the circle, coloured by source book, and Old↔New Testament links highlighted.

There is no build system, package, or test suite. The whole program is one file.

## Commands

```bash
pip install matplotlib                      # only third-party dependency
python bible_circle.py nwtsty_X.jwpub       # explicit input
python bible_circle.py                      # auto-finds the first *.jwpub next to the script
python bible_circle.py --lang en            # English labels/UI/CSV headers (default: de)
python bible_circle.py --formats png-sm,html  # produce only some outputs (default: all)
```

`argparse` flags (see `parse_args()` / `resolve_formats()`):
- `--lang {de,en}` selects the active `STRINGS[...]` entry into the module global `S` (book names,
  abbreviations, titles, OT/NT labels, HTML UI strings, CSV header). Default `de`.
- `--formats` is a comma-separated subset of `ALL_FORMATS = [db, csv, svg, png, png-sm, html]`, or
  `all` (default). It just sets the `CFG["WRITE_*"]` toggles. `db` controls whether the extracted
  `.db` is kept (it's always extracted, then deleted unless requested).

Outputs are written **next to the input `.jwpub`** (i.e. into this folder):
`nwtsty_X.db`, `bible_crossrefs.csv`, `bible_circle.svg`, `bible_circle.png`,
`bible_circle_sm.png` (small cropped README thumbnail, re-rendered at decreasing DPI until it fits
`CFG["PNG_SM_MAX_BYTES"]`), `bible_circle.html`.

To regenerate after a Bible update: download the new `.jwpub` (JWPUB format of the *Studienausgabe*
from <https://www.jw.org/de/bibliothek/bibel/>), drop it here, rerun the command.

## The data source (important — no decryption needed)

A `.jwpub` is a ZIP containing `manifest.json` + a `contents` blob. **`contents` is itself a
plain ZIP** (PK header, not encrypted) holding a ~49 MB plaintext **SQLite** database
(`nwtsty_X.db`). `extract_db()` does outer-unzip → read `manifest.json` for the db filename →
inner-unzip → write the `.db`.

Relevant tables (queried in `Bible.__init__`):

- **`BibleCitation`** — the cross-references = the graph edges.
  `BibleVerseId` = source verse; `FirstBibleVerseId`/`LastBibleVerseId` = target verse range
  (the code uses the midpoint); `MarginalClassification` (0–3) = reference type. Only rows with
  a non-null source and target are used (~65,600 edges; ~7,570 are Old↔New).
- **`BibleBook`** (66 rows) — `FirstVerseId`/`LastVerseId` per book. Books 1–39 = Old Testament,
  40–66 = New Testament.
- **`BibleVerse`** — every verse has a **global linear index `BibleVerseId` (0–31193)**, used as
  the angular coordinate on the ring.
- **`BibleChapter`** (`BookNumber`, `ChapterNumber`, `FirstVerseId`, `LastVerseId`) — used to turn
  a verse id back into a label like `Jes 7:14`.

This particular publication is the **German** Study Bible (Studienbibel). The `.jwpub` supplies
only verse *ids*; book names and all UI text come from the `STRINGS` table, so `--lang` only
relabels the graphic (default German) — it does not translate any Bible text.

## Architecture / data flow

`main()` runs a linear pipeline:

1. **`extract_db()`** — unzip-in-unzip → SQLite file.
2. **`Bible`** (class) — loads books/chapters/edges, then **`_layout_books()`** assigns each book an
   angular wedge. **`verse_angle(vid)`** interpolates a verse's angle within its book's wedge.
   **`verse_label(vid)`** → `"Abbr C:V"`.
3. **`write_csv()`** — the edge list (`bible_crossrefs.csv`).
4. **`build_figure(bible, draw_text)`** — the shared matplotlib renderer. Draws the faint
   all-edges layer + the highlighted Old↔New layer + a coloured rim arc per book. With
   `draw_text=True` it also adds the book-name labels, testament labels and title.
   - **`render_static()`** calls it with `draw_text=True` → saves SVG + PNG.
   - **`render_html()`** calls it with `draw_text=False` → a clean, label-free PNG embedded
     (base64) as the background of a self-contained HTML file, then overlays an SVG of the
     Old↔New chords (interactive, `<title>` tooltips) plus crisp vector book labels. Vanilla JS
     gives wheel-zoom, drag-pan, and an "Old↔New only" toggle (which hides the background image).

## Layout logic (where the visual structure lives)

- The Old Testament fills the **left half** of the circle `[90°+GAP .. 270°−GAP]`; the New
  Testament fills the **right half** `[270°+GAP .. 450°−GAP]`. The two seam gaps land at top
  (Offenbarung↔1.Mose) and bottom (Maleachi↔Matthäus).
- Within each half, every book gets a wedge **proportional to its verse count**, separated by
  `BOOK_GAP_DEG`. The New Testament adds `BRACKET_GAP_DEG` **before Matthäus** and **after
  Johannes** to set the four Gospels apart as a cluster.
- Chords are quadratic Béziers with the control point pulled toward the centre by `BOW`.

## Tuning — everything lives in the `CFG` dict at the top of the file

Geometry: `R`, `L` (canvas half-extent — increase if long labels clip), `GAP_DEG` (testament
seams), `BOOK_GAP_DEG`, `BRACKET_GAP_DEG` (Gospel bracket), `BOW` (chord bend).
Layers: `FAINT_ALPHA`/`FAINT_LW` (all edges), `HL_ALPHA`/`HL_LW` (Old↔New highlight).
Labels/render: `LABEL_FONTSIZE`, `FIG_INCHES`, `DPI`.
Small thumbnail: `PNG_SM_MAX_BYTES` (byte budget), `PNG_SM_DPIS` (DPI ladder tried high→low until
under budget), `PNG_SM_PAD_INCHES` (crop padding).
Outputs: `WRITE_DB`/`WRITE_CSV`/`WRITE_SVG`/`WRITE_PNG`/`WRITE_PNG_SM`/`WRITE_HTML` toggles (set
from `--formats`) + `OUT_*` filenames.

Localization is a separate `STRINGS` dict (keyed by language, default selected into global `S`):
per-language `abbr` (compact abbreviations used in CSV/verse labels), `name` (full ring labels),
`title`/`subtitle`, `ot`/`nt`, the HTML UI strings (`toggle`/`reset`/`hint`/`html_lang`) and
`csv_header`. `N_OT_BOOKS = 39` (module-level) defines the language-independent OT/NT split.

## Gotchas

- **Windows console is cp1252** and chokes on `↔`/`→`. The script reconfigures `stdout`/`stderr`
  to UTF-8 at startup; all output files are written UTF-8. If you see `1K�` in a terminal it's a
  console display artifact, not corrupt data.
- **Coordinate transforms:** matplotlib uses y-up; SVG uses y-down. `render_html()` negates y
  (`sy()`) and rotation angles. The matplotlib `Arc` patch and `verse_angle` share the same
  degrees-CCW-from-+x convention, which is what keeps the PNG background aligned with the SVG overlay.
- The SVG/PNG contain ~65k chord paths, so the SVG is large (~16 MB) and slow to open in editors;
  that's expected. The HTML keeps only the ~7.5k Old↔New chords interactive (background is a raster).
