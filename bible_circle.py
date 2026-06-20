#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
bible_circle.py
===============
Build a CIRCULAR cross-reference graphic from a JW Library Study Bible (.jwpub),
inspired by the "All Prophecies about Jesus" arc diagram -- but bent into a circle.

The 66 Bible books are laid out around a ring (Old Testament down the left half,
New Testament up the right half). Every cross-reference in the study Bible is drawn
as a chord bowing through the circle, coloured by its SOURCE book (rainbow).
Cross-references that link the Old and New Testaments are highlighted on top.

No decryption is required: a .jwpub is a zip containing a 'contents' blob that is
itself a zip holding a plaintext SQLite database (nwtsty_X.db). All cross-references
live in the table `BibleCitation`.

Outputs (written next to the .jwpub, i.e. into your Downloads folder):
    nwtsty_X.db            extracted SQLite database
    bible_crossrefs.csv    every edge: source ref, target ref, books, type
    bible_circle.svg       vector graphic
    bible_circle.png       high-res raster
    bible_circle.html      self-contained interactive viewer (zoom/pan/hover/toggle)

Usage:
    pip install matplotlib
    python bible_circle.py nwtsty_X.jwpub
    # or just:  python bible_circle.py        (auto-finds a .jwpub in this folder)

Re-run after downloading a Bible update to regenerate everything.
"""

import sys
import os
import io
import csv
import json
import math
import base64
import zipfile
import sqlite3
import bisect
import glob

# Make console output UTF-8 safe on Windows (cp1252 can't encode ↔ / →)
try:
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
except Exception:
    pass

# ----------------------------------------------------------------------------
# CONFIG  -- tweak these knobs and re-run
# ----------------------------------------------------------------------------
CFG = {
    # geometry
    "R": 1.0,            # ring radius (chord endpoints sit here)
    "L": 1.78,           # half-extent of the canvas (room for the long book names)
    "GAP_DEG": 7.0,      # gap (degrees) at top (Off|1Mo) and bottom (Mal|Mat) seams
    "BOOK_GAP_DEG": 0.7,     # gap between every adjacent book (visual separation)
    "BRACKET_GAP_DEG": 4.5,  # extra gap before Matthäus and after Johannes (brackets the Gospels)
    "BOW": 0.32,         # chord control-point pull toward centre (0=through centre, 1=straight)

    # faint background layer (ALL cross-references)
    "FAINT_ALPHA": 0.06,
    "FAINT_LW": 0.20,

    # highlighted layer (Old <-> New Testament cross-references)
    "HL_ALPHA": 0.55,
    "HL_LW": 0.55,

    # labels / render
    "LABEL_FONTSIZE": 5.0,
    "TITLE": "Bibel-Querverweise",
    "SUBTITLE": "Alle Randverweise der NWT-Studienbibel — eingefärbt nach Ausgangsbuch; "
                "Verweise zwischen Altem und Neuem Testament hervorgehoben",
    "FIG_INCHES": 13.0,
    "DPI": 300,

    # which outputs to produce
    "WRITE_DB": True,
    "WRITE_CSV": True,
    "WRITE_SVG": True,
    "WRITE_PNG": True,
    "WRITE_HTML": True,

    # output filenames (created in the output dir)
    "OUT_CSV": "bible_crossrefs.csv",
    "OUT_SVG": "bible_circle.svg",
    "OUT_PNG": "bible_circle.png",
    "OUT_HTML": "bible_circle.html",
}

# Offizielle Buch-Abkürzungen der Neue-Welt-Übersetzung (deutsch), 66 Bücher in Reihenfolge.
BOOK_ABBR = [
    "1Mo", "2Mo", "3Mo", "4Mo", "5Mo", "Jos", "Ri", "Ru", "1Sa", "2Sa",
    "1Kö", "2Kö", "1Ch", "2Ch", "Esr", "Ne", "Est", "Hi", "Ps", "Spr",
    "Pr", "Hoh", "Jes", "Jer", "Klg", "Hes", "Da", "Hos", "Joe", "Am",
    "Ob", "Jon", "Mi", "Na", "Hab", "Zef", "Hag", "Sach", "Mal",
    "Mat", "Mar", "Luk", "Joh", "Apg", "Rö", "1Ko", "2Ko", "Gal", "Eph",
    "Php", "Kol", "1Th", "2Th", "1Ti", "2Ti", "Tit", "Phm", "Heb", "Jak",
    "1Pe", "2Pe", "1Jo", "2Jo", "3Jo", "Jud", "Off",
]

# Volle deutsche Buchnamen (für die Beschriftungen am Ring), 66 Bücher in Reihenfolge.
BOOK_NAME = [
    "1. Mose", "2. Mose", "3. Mose", "4. Mose", "5. Mose", "Josua", "Richter", "Ruth",
    "1. Samuel", "2. Samuel", "1. Könige", "2. Könige", "1. Chronika", "2. Chronika",
    "Esra", "Nehemia", "Esther", "Hiob", "Psalmen", "Sprüche", "Prediger", "Hoheslied",
    "Jesaja", "Jeremia", "Klagelieder", "Hesekiel", "Daniel", "Hosea", "Joel", "Amos",
    "Obadja", "Jona", "Micha", "Nahum", "Habakuk", "Zephanja", "Haggai", "Sacharja", "Maleachi",
    "Matthäus", "Markus", "Lukas", "Johannes", "Apostelgeschichte", "Römer",
    "1. Korinther", "2. Korinther", "Galater", "Epheser", "Philipper", "Kolosser",
    "1. Thessalonicher", "2. Thessalonicher", "1. Timotheus", "2. Timotheus", "Titus",
    "Philemon", "Hebräer", "Jakobus", "1. Petrus", "2. Petrus",
    "1. Johannes", "2. Johannes", "3. Johannes", "Judas", "Offenbarung",
]
N_OT_BOOKS = 39  # Bücher 1..39 gehören zum Alten Testament (Hebräische Schriften)


# ----------------------------------------------------------------------------
# 1. Extract the SQLite DB from the .jwpub (zip-in-zip, no decryption)
# ----------------------------------------------------------------------------
def extract_db(jwpub_path, out_dir):
    print(f"[1/6] Opening {os.path.basename(jwpub_path)} ...")
    with zipfile.ZipFile(jwpub_path) as outer:
        manifest = json.loads(outer.read("manifest.json"))
        db_name = manifest["publication"]["fileName"]
        print(f"      manifest -> publication db: {db_name}")
        contents = outer.read("contents")            # inner zip (plaintext)
    with zipfile.ZipFile(io.BytesIO(contents)) as inner:
        db_bytes = inner.read(db_name)
    db_path = os.path.join(out_dir, db_name)
    with open(db_path, "wb") as f:
        f.write(db_bytes)
    print(f"      extracted DB -> {db_path}  ({len(db_bytes)/1e6:.1f} MB)")
    return db_path


# ----------------------------------------------------------------------------
# 2. Load books, chapters and cross-reference edges
# ----------------------------------------------------------------------------
class Bible:
    def __init__(self, db_path):
        print("[2/6] Reading Bible structure and cross-references ...")
        con = sqlite3.connect(db_path)
        q = con.execute

        # --- books ---
        rows = q("SELECT BibleBookId, FirstVerseId, LastVerseId "
                 "FROM BibleBook ORDER BY BibleBookId").fetchall()
        self.books = []            # index 0..65
        self._book_first = []      # for bisect (vid -> book)
        for bid, first, last in rows:
            idx = bid - 1
            abbr = BOOK_ABBR[idx] if idx < len(BOOK_ABBR) else f"B{bid}"
            name = BOOK_NAME[idx] if idx < len(BOOK_NAME) else abbr
            self.books.append({
                "id": bid, "idx": idx, "abbr": abbr, "name": name,
                "first": first, "last": last,
                "testament": "OT" if bid <= N_OT_BOOKS else "NT",
                "hue": idx / len(rows),
            })
            self._book_first.append(first)

        # global verse range and the OT/NT split
        self.v_min = self.books[0]["first"]
        self.v_max = self.books[-1]["last"]
        self.nt_start = self.books[N_OT_BOOKS]["first"]   # first verse of book 40 (Matthew)

        # per-book angular layout (with gaps between books) -> sets a0/a1/mid degrees
        self._layout_books()

        # --- chapters (for human-readable verse labels) ---
        ch = q("SELECT BookNumber, ChapterNumber, FirstVerseId, LastVerseId "
               "FROM BibleChapter ORDER BY FirstVerseId").fetchall()
        self._ch_first = [r[2] for r in ch]
        self._chapters = ch

        # --- cross-reference edges ---
        cites = q("SELECT BibleVerseId, FirstBibleVerseId, LastBibleVerseId, "
                  "MarginalClassification FROM BibleCitation "
                  "WHERE BibleVerseId IS NOT NULL "
                  "AND FirstBibleVerseId IS NOT NULL").fetchall()
        con.close()

        self.edges = []
        for src, tfirst, tlast, cls in cites:
            tgt = (tfirst + tlast) // 2
            sb = self.book_idx_of(src)
            tb = self.book_idx_of(tgt)
            if sb is None or tb is None:
                continue
            cross = (src < self.nt_start) != (tgt < self.nt_start)
            self.edges.append((src, tgt, sb, tb, int(cls), cross))

        n_cross = sum(1 for e in self.edges if e[5])
        print(f"      books: {len(self.books)}   verses: {self.v_min}..{self.v_max}")
        print(f"      edges: {len(self.edges)}   Old↔New Testament: {n_cross}")

    # vid -> book index (0..65)
    def book_idx_of(self, vid):
        i = bisect.bisect_right(self._book_first, vid) - 1
        if 0 <= i < len(self.books) and self.books[i]["first"] <= vid <= self.books[i]["last"]:
            return i
        return None

    # vid -> "Abbr C:V"
    def verse_label(self, vid):
        bi = self.book_idx_of(vid)
        abbr = self.books[bi]["abbr"] if bi is not None else "?"
        ci = bisect.bisect_right(self._ch_first, vid) - 1
        if 0 <= ci < len(self._chapters):
            _, chap, cfirst, clast = self._chapters[ci]
            if cfirst <= vid <= clast:
                return f"{abbr} {chap}:{vid - cfirst + 1}"
        return abbr

    # Lay each book out as its own angular wedge, with gaps between books.
    # OT fills the left half [90+GAP .. 270-GAP]; NT fills the right half
    # [270+GAP .. 450-GAP]. Wedge width is proportional to a book's verse count.
    def _layout_books(self):
        seam = CFG["GAP_DEG"]
        bgap = CFG["BOOK_GAP_DEG"]
        bracket = CFG["BRACKET_GAP_DEG"]
        ot = [b for b in self.books if b["testament"] == "OT"]
        nt = [b for b in self.books if b["testament"] == "NT"]

        def lay(books, start_deg, end_deg, gap_before):
            # gap_before[i] = degrees of empty space inserted before book i
            total_span = end_deg - start_deg
            total_gap = sum(gap_before)
            usable = total_span - total_gap
            total_verses = sum(max(1, b["last"] - b["first"] + 1) for b in books)
            cursor = start_deg
            for i, b in enumerate(books):
                cursor += gap_before[i]
                w = usable * max(1, b["last"] - b["first"] + 1) / total_verses
                b["a0"] = cursor
                b["a1"] = cursor + w
                b["mid"] = cursor + w / 2.0
                cursor = b["a1"]

        # OT: a simple gap between every adjacent book
        ot_gaps = [0.0] + [bgap] * (len(ot) - 1)
        lay(ot, 90 + seam, 270 - seam, ot_gaps)

        # NT: gap between books, PLUS extra space before Matthäus (leading)
        # and after Johannes (i.e. before Apostelgeschichte) -> brackets the Gospels.
        nt_gaps = [bgap] * len(nt)
        nt_gaps[0] = bracket                       # before Matthäus (NT-local index 0)
        if len(nt) > 4:
            nt_gaps[4] = bgap + bracket            # before Apostelgeschichte (after Johannes)
        lay(nt, 270 + seam, 450 - seam, nt_gaps)

    # vid -> angle on the ring, in radians (math convention, y up)
    def verse_angle(self, vid):
        bi = self.book_idx_of(vid)
        b = self.books[bi] if bi is not None else self.books[0]
        span = max(1, b["last"] - b["first"])
        frac = min(1.0, max(0.0, (vid - b["first"]) / span))
        return math.radians(b["a0"] + frac * (b["a1"] - b["a0"]))

    def verse_xy(self, vid, r=None):
        r = CFG["R"] if r is None else r
        a = self.verse_angle(vid)
        return r * math.cos(a), r * math.sin(a)


# ----------------------------------------------------------------------------
# 3. CSV export
# ----------------------------------------------------------------------------
def write_csv(bible, path):
    print(f"[3/6] Writing edge list -> {os.path.basename(path)} ...")
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["quelle_vers", "ziel_vers", "quelle_buch", "ziel_buch",
                    "klassifikation", "testament_uebergreifend"])
        for src, tgt, sb, tb, cls, cross in bible.edges:
            w.writerow([bible.verse_label(src), bible.verse_label(tgt),
                        bible.books[sb]["abbr"], bible.books[tb]["abbr"],
                        cls, int(cross)])
    print(f"      wrote {len(bible.edges)} rows")


# ----------------------------------------------------------------------------
# helpers shared by the renderers
# ----------------------------------------------------------------------------
def chord_control(x1, y1, x2, y2):
    """Quadratic Bezier control point: midpoint pulled toward the centre."""
    bow = CFG["BOW"]
    return (x1 + x2) * 0.5 * bow, (y1 + y2) * 0.5 * bow


def book_hue_rgb(hue):
    import colorsys
    r, g, b = colorsys.hsv_to_rgb(hue, 0.80, 0.85)
    return r, g, b


# ----------------------------------------------------------------------------
# 4./5. Static render (SVG + PNG) via matplotlib
# ----------------------------------------------------------------------------
def build_figure(bible, draw_text=True):
    """Build the figure: chords + coloured book rim arcs (always); the book name
    labels, testament labels and title only when draw_text=True (the HTML uses a
    label-free background so its overlay can supply crisp, zoomable labels)."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.path import Path
    from matplotlib.collections import PathCollection
    from matplotlib.patches import Arc

    L = CFG["L"]
    S = CFG["FIG_INCHES"]
    R = CFG["R"]
    fig = plt.figure(figsize=(S, S), facecolor="white")
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_xlim(-L, L)
    ax.set_ylim(-L, L)
    ax.set_aspect("equal")
    ax.axis("off")

    def build(paths_edges):
        paths, colors = [], []
        for src, tgt, sb, tb, cls, cross in paths_edges:
            x1, y1 = bible.verse_xy(src)
            x2, y2 = bible.verse_xy(tgt)
            cx, cy = chord_control(x1, y1, x2, y2)
            paths.append(Path([(x1, y1), (cx, cy), (x2, y2)],
                              [Path.MOVETO, Path.CURVE3, Path.CURVE3]))
            colors.append(book_hue_rgb(bible.books[sb]["hue"]))
        return paths, colors

    # faint background: ALL edges
    fp, fc = build(bible.edges)
    ax.add_collection(PathCollection(fp, facecolors="none", edgecolors=fc,
                                     linewidths=CFG["FAINT_LW"],
                                     alpha=CFG["FAINT_ALPHA"], zorder=1,
                                     capstyle="round"))
    # highlight: OT<->NT edges on top
    hp, hc = build([e for e in bible.edges if e[5]])
    ax.add_collection(PathCollection(hp, facecolors="none", edgecolors=hc,
                                     linewidths=CFG["HL_LW"],
                                     alpha=CFG["HL_ALPHA"], zorder=3,
                                     capstyle="round"))
    if draw_text:
        print(f"      drew {len(fp)} faint chords, {len(hp)} highlighted Old↔New chords")

    # coloured rim arc per book (gaps between books make each one distinct)
    for b in bible.books:
        ax.add_patch(Arc((0, 0), 2 * R, 2 * R, theta1=b["a0"], theta2=b["a1"],
                         color=book_hue_rgb(b["hue"]), lw=3.0, zorder=4,
                         capstyle="butt"))

    if draw_text:
        for b in bible.books:
            a = math.radians(b["mid"])
            col = book_hue_rgb(b["hue"])
            deg = b["mid"] % 360
            lx, ly = (R + 0.045) * math.cos(a), (R + 0.045) * math.sin(a)
            rot, ha = deg, "left"
            if 90 < deg < 270:
                rot, ha = deg - 180, "right"
            ax.text(lx, ly, b["name"], fontsize=CFG["LABEL_FONTSIZE"], color=col,
                    rotation=rot, rotation_mode="anchor", ha=ha, va="center",
                    zorder=5, fontweight="bold")

        ax.text(-(R + 0.13), 0, "ALTES TESTAMENT", rotation=90, ha="center",
                va="center", fontsize=10, color="#444", fontweight="bold", zorder=6)
        ax.text((R + 0.13), 0, "NEUES TESTAMENT", rotation=-90, ha="center",
                va="center", fontsize=10, color="#444", fontweight="bold", zorder=6)
        ax.text(0, L - 0.05, CFG["TITLE"], ha="center", va="top",
                fontsize=17, color="#222", fontweight="bold", zorder=6)
        ax.text(0, L - 0.13, CFG["SUBTITLE"], ha="center", va="top",
                fontsize=8.0, color="#666", zorder=6)
    return fig


def render_static(bible, svg_path, png_path):
    print("[4/6] Rendering static graphic (matplotlib) ...")
    import matplotlib.pyplot as plt
    fig = build_figure(bible, draw_text=True)
    if CFG["WRITE_SVG"]:
        fig.savefig(svg_path, format="svg")
        print(f"      wrote {svg_path}")
    if CFG["WRITE_PNG"]:
        fig.savefig(png_path, format="png", dpi=CFG["DPI"])
        print(f"      wrote {png_path}  ({CFG['DPI']} dpi)")
    plt.close(fig)


# ----------------------------------------------------------------------------
# 6. Interactive HTML  (faint PNG background + interactive SVG overlay)
# ----------------------------------------------------------------------------
def render_html(bible, html_path):
    print("[5/6] Building interactive HTML ...")
    import matplotlib.pyplot as plt
    L = CFG["L"]               # viewBox half-extent (whole canvas incl. labels)
    Lpng = CFG["L"]            # the PNG canvas half-extent (for background placement)

    # clean, label-free background (chords + coloured book arcs) for crisp overlay labels
    bg = build_figure(bible, draw_text=False)
    buf = io.BytesIO()
    bg.savefig(buf, format="png", dpi=200)
    plt.close(bg)
    bg_b64 = base64.b64encode(buf.getvalue()).decode("ascii")

    # world (y up) -> svg (y down)
    def sx(x):
        return f"{x:.4f}"

    def sy(y):
        return f"{-y:.4f}"

    # build interactive OT<->NT chords
    chords = []
    for src, tgt, sb, tb, cls, cross in bible.edges:
        if not cross:
            continue
        x1, y1 = bible.verse_xy(src)
        x2, y2 = bible.verse_xy(tgt)
        cx, cy = chord_control(x1, y1, x2, y2)
        r, g, b = book_hue_rgb(bible.books[sb]["hue"])
        col = f"#{int(r*255):02x}{int(g*255):02x}{int(b*255):02x}"
        d = f"M{sx(x1)},{sy(y1)} Q{sx(cx)},{sy(cy)} {sx(x2)},{sy(y2)}"
        tip = f"{bible.verse_label(src)} → {bible.verse_label(tgt)}"
        chords.append(
            f'<path class="c" d="{d}" stroke="{col}" '
            f'fill="none" stroke-width="0.004" stroke-opacity="0.7">'
            f'<title>{tip}</title></path>'
        )
    print(f"      {len(chords)} interactive Old↔New chords")

    # book labels (full German names, crisp vector text)
    R = CFG["R"]
    labels = []
    for bk in bible.books:
        a = math.radians(bk["mid"])
        rr, gg, bb = book_hue_rgb(bk["hue"])
        col = f"#{int(rr*255):02x}{int(gg*255):02x}{int(bb*255):02x}"
        lx, ly = (R + 0.04) * math.cos(a), (R + 0.04) * math.sin(a)
        deg = bk["mid"] % 360
        rot = deg
        anchor = "start"
        if 90 < deg < 270:
            rot = deg - 180
            anchor = "end"
        # svg rotation is clockwise & y is flipped -> negate angle
        labels.append(
            f'<text x="{sx(lx)}" y="{sy(ly)}" fill="{col}" '
            f'font-size="0.026" font-weight="bold" text-anchor="{anchor}" '
            f'dominant-baseline="middle" '
            f'transform="rotate({-rot:.2f} {sx(lx)} {sy(ly)})">{bk["name"]}</text>'
        )

    # base64 PNG background (clean, label-free)
    img_tag = (f'<image id="bg" x="{sx(-Lpng)}" y="{sy(Lpng)}" '
               f'width="{2*Lpng:.4f}" height="{2*Lpng:.4f}" '
               f'href="data:image/png;base64,{bg_b64}" '
               f'preserveAspectRatio="none"/>')

    vb = f"{-L:.3f} {-L:.3f} {2*L:.3f} {2*L:.3f}"
    title = CFG["TITLE"]
    subtitle = CFG["SUBTITLE"]

    html = f"""<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ margin:0; font-family: system-ui, "Segoe UI", Arial, sans-serif;
         background:#fafafa; color:#222; }}
  header {{ padding:14px 18px 6px; }}
  h1 {{ font-size:18px; margin:0; }}
  p.sub {{ font-size:12px; color:#666; margin:4px 0 0; }}
  #toolbar {{ padding:8px 18px; font-size:13px; display:flex; gap:18px;
             align-items:center; flex-wrap:wrap; }}
  #wrap {{ width:100vw; height:calc(100vh - 110px); overflow:hidden;
           background:#fff; border-top:1px solid #eee; cursor:grab; }}
  #wrap.drag {{ cursor:grabbing; }}
  svg {{ width:100%; height:100%; display:block; touch-action:none; }}
  .c:hover {{ stroke-width:0.012 !important; stroke-opacity:1 !important; }}
  button {{ font-size:13px; padding:3px 10px; cursor:pointer; }}
  .hint {{ color:#888; }}
</style>
</head>
<body>
<header>
  <h1>{title}</h1>
  <p class="sub">{subtitle}</p>
</header>
<div id="toolbar">
  <label><input type="checkbox" id="otnt"> Nur Verweise zwischen Altem und Neuem Testament zeigen</label>
  <button id="reset">Ansicht zurücksetzen</button>
  <span class="hint">scrollen = zoomen &nbsp;·&nbsp; ziehen = verschieben &nbsp;·&nbsp; über eine Linie fahren = Verse</span>
</div>
<div id="wrap">
  <svg id="svg" viewBox="{vb}" preserveAspectRatio="xMidYMid meet">
    <g id="bglayer">{img_tag}</g>
    <g id="chords">
      {''.join(chords)}
    </g>
    <g id="labels">
      {''.join(labels)}
    </g>
  </svg>
</div>
<script>
  const svg = document.getElementById('svg');
  const wrap = document.getElementById('wrap');
  const VB0 = [{-L:.3f}, {-L:.3f}, {2*L:.3f}, {2*L:.3f}];
  let vb = VB0.slice();
  function apply() {{ svg.setAttribute('viewBox', vb.join(' ')); }}

  // zoom toward cursor
  wrap.addEventListener('wheel', (e) => {{
    e.preventDefault();
    const r = svg.getBoundingClientRect();
    const mx = vb[0] + (e.clientX - r.left)/r.width  * vb[2];
    const my = vb[1] + (e.clientY - r.top )/r.height * vb[3];
    const k = Math.exp((e.deltaY||0) * 0.0015);
    vb[2]*=k; vb[3]*=k;
    vb[0] = mx - (e.clientX - r.left)/r.width  * vb[2];
    vb[1] = my - (e.clientY - r.top )/r.height * vb[3];
    apply();
  }}, {{passive:false}});

  // drag to pan
  let drag=false, px=0, py=0;
  wrap.addEventListener('pointerdown', (e)=>{{drag=true;px=e.clientX;py=e.clientY;
       wrap.classList.add('drag'); wrap.setPointerCapture(e.pointerId);}});
  wrap.addEventListener('pointermove', (e)=>{{ if(!drag) return;
       const r=svg.getBoundingClientRect();
       vb[0]-=(e.clientX-px)/r.width *vb[2];
       vb[1]-=(e.clientY-py)/r.height*vb[3];
       px=e.clientX; py=e.clientY; apply(); }});
  function endDrag(){{drag=false; wrap.classList.remove('drag');}}
  wrap.addEventListener('pointerup', endDrag);
  wrap.addEventListener('pointercancel', endDrag);

  document.getElementById('reset').onclick=()=>{{vb=VB0.slice();apply();}};
  document.getElementById('otnt').onchange=(e)=>{{
    document.getElementById('bglayer').style.display = e.target.checked ? 'none':'';
  }};
  apply();
</script>
</body>
</html>
"""
    with open(html_path, "w", encoding="utf-8") as f:
        f.write(html)
    print(f"      wrote {html_path}  ({len(html)/1e6:.1f} MB)")


# ----------------------------------------------------------------------------
# main
# ----------------------------------------------------------------------------
def find_jwpub():
    if len(sys.argv) > 1:
        return sys.argv[1]
    here = os.path.dirname(os.path.abspath(__file__))
    cands = glob.glob(os.path.join(here, "*.jwpub"))
    if not cands:
        cands = glob.glob("*.jwpub")
    if not cands:
        sys.exit("No .jwpub given and none found in this folder.\n"
                 "Usage: python bible_circle.py nwtsty_X.jwpub")
    return cands[0]


def main():
    jwpub = os.path.abspath(find_jwpub())
    out_dir = os.path.dirname(jwpub)
    print(f"Input : {jwpub}")
    print(f"Output: {out_dir}\n")

    db_path = extract_db(jwpub, out_dir)
    bible = Bible(db_path)

    if CFG["WRITE_CSV"]:
        write_csv(bible, os.path.join(out_dir, CFG["OUT_CSV"]))

    svg_path = os.path.join(out_dir, CFG["OUT_SVG"])
    png_path = os.path.join(out_dir, CFG["OUT_PNG"])
    if CFG["WRITE_SVG"] or CFG["WRITE_PNG"]:
        render_static(bible, svg_path, png_path)

    if CFG["WRITE_HTML"]:
        render_html(bible, os.path.join(out_dir, CFG["OUT_HTML"]))

    if not CFG["WRITE_DB"]:
        try:
            os.remove(db_path)
        except OSError:
            pass

    print("\n[6/6] Done. Files written to:", out_dir)


if __name__ == "__main__":
    main()
