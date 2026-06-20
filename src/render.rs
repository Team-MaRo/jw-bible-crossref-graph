//! CSV / SVG / PNG / HTML renderers. The SVG is hand-built as a string (y-down:
//! world y is negated on output) and rasterized to PNG via resvg/tiny-skia.

use crate::bible::{book_hue_rgb, cfg, chord_control, hex, Bible, Edge};
use anyhow::{Context, Result};
use base64::Engine as _;
use std::borrow::Cow;
use std::fmt::Write as _;
use std::path::Path;

// matplotlib point sizes converted to world units (so strokes match the original
// at 300 dpi: 0.2pt faint ≈ 0.83px, 0.55pt highlight ≈ 2.3px, 3pt rim ≈ 12.5px).
const FAINT_W: f64 = cfg::FAINT_LW * cfg::PT2WORLD;
const HL_W: f64 = cfg::HL_LW * cfg::PT2WORLD;
const RIM_W: f64 = cfg::RIM_LW * cfg::PT2WORLD;
const LABEL_FS: f64 = cfg::LABEL_FONTSIZE * cfg::PT2WORLD;
const TITLE_FS: f64 = 17.0 * cfg::PT2WORLD;
const SUB_FS: f64 = 8.0 * cfg::PT2WORLD;
const TNT_FS: f64 = 10.0 * cfg::PT2WORLD;

fn xesc(s: &str) -> Cow<'_, str> {
    if s.bytes().any(|b| matches!(b, b'&' | b'<' | b'>')) {
        Cow::Owned(
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;"),
        )
    } else {
        Cow::Borrowed(s)
    }
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------
pub fn write_csv(bible: &Bible, path: &Path) -> Result<()> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    println!("[3/7] Writing edge list -> {name} ...");
    let s = bible.lang.strings();
    let mut out = String::with_capacity(bible.edges.len() * 24);
    out.push_str(&s.csv_header.join(","));
    out.push('\n');
    for e in &bible.edges {
        // verse labels and abbreviations never contain commas
        writeln!(
            out,
            "{},{},{},{},{},{}",
            bible.verse_label(e.src),
            bible.verse_label(e.tgt),
            bible.books[e.sb].abbr,
            bible.books[e.tb].abbr,
            e.cls,
            e.cross as i32
        )
        .unwrap();
    }
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))?;
    println!("      wrote {} rows", bible.edges.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared SVG builder
// ---------------------------------------------------------------------------
/// Build the full graphic as an SVG string: faint all-edges layer + highlighted
/// Old<->New layer + a coloured rim arc per book, plus labels/title when
/// `draw_text` is set. Coordinates are emitted y-down (world y negated).
pub fn build_svg(bible: &Bible, draw_text: bool) -> String {
    let l = cfg::L;
    let r = cfg::R;
    let px = (cfg::FIG_INCHES * cfg::DPI) as i64; // 3900 -> scale 1.0 == 300 dpi
    let colors: Vec<String> = bible
        .books
        .iter()
        .map(|b| hex(book_hue_rgb(b.hue)))
        .collect();

    let mut s = String::with_capacity(16 * 1024 * 1024);
    let _ = write!(
        s,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="{:.3} {:.3} {:.3} {:.3}">"##,
        -l,
        -l,
        2.0 * l,
        2.0 * l
    );
    let _ = write!(
        s,
        r##"<rect x="{:.3}" y="{:.3}" width="{:.3}" height="{:.3}" fill="#ffffff"/>"##,
        -l,
        -l,
        2.0 * l,
        2.0 * l
    );

    // faint background: ALL edges
    s.push_str(r##"<g fill="none" stroke-linecap="round">"##);
    for e in &bible.edges {
        append_chord(&mut s, bible, e, &colors[e.sb], FAINT_W, cfg::FAINT_ALPHA);
    }
    s.push_str("</g>");

    // highlight: Old<->New edges on top
    s.push_str(r##"<g fill="none" stroke-linecap="round">"##);
    for e in bible.edges.iter().filter(|e| e.cross) {
        append_chord(&mut s, bible, e, &colors[e.sb], HL_W, cfg::HL_ALPHA);
    }
    s.push_str("</g>");

    // coloured rim arc per book
    for b in &bible.books {
        let (x0, y0) = (r * b.a0.to_radians().cos(), r * b.a0.to_radians().sin());
        let (x1, y1) = (r * b.a1.to_radians().cos(), r * b.a1.to_radians().sin());
        // sweep-flag 0: after the y-negation, this is the arc centred on the
        // origin (sweep 1 picks the mirror-centred circle and bows the wrong way).
        let _ = write!(
            s,
            r##"<path d="M{:.4},{:.4} A{:.4},{:.4} 0 0 0 {:.4},{:.4}" fill="none" stroke="{}" stroke-width="{:.5}"/>"##,
            x0, -y0, r, r, x1, -y1, colors[b.idx], RIM_W
        );
    }

    if draw_text {
        append_text(&mut s, bible, &colors);
    }
    s.push_str("</svg>");
    s
}

fn append_chord(s: &mut String, bible: &Bible, e: &Edge, col: &str, w: f64, alpha: f64) {
    let (x1, y1) = bible.verse_xy(e.src, cfg::R);
    let (x2, y2) = bible.verse_xy(e.tgt, cfg::R);
    let (cx, cy) = chord_control(x1, y1, x2, y2);
    let _ = write!(
        s,
        r##"<path d="M{:.4},{:.4} Q{:.4},{:.4} {:.4},{:.4}" stroke="{}" stroke-width="{:.5}" stroke-opacity="{}"/>"##,
        x1, -y1, cx, -cy, x2, -y2, col, w, alpha
    );
}

fn append_text(s: &mut String, bible: &Bible, colors: &[String]) {
    let r = cfg::R;
    let l = cfg::L;
    let st = bible.lang.strings();

    // per-book ring labels
    for b in &bible.books {
        let a = b.mid.to_radians();
        let lx = (r + 0.04) * a.cos();
        let ly = (r + 0.04) * a.sin();
        let deg = b.mid.rem_euclid(360.0);
        let (rot, anchor) = if deg > 90.0 && deg < 270.0 {
            (deg - 180.0, "end")
        } else {
            (deg, "start")
        };
        let _ = write!(
            s,
            r##"<text x="{:.4}" y="{:.4}" fill="{}" font-size="{:.4}" font-weight="bold" text-anchor="{}" dominant-baseline="middle" transform="rotate({:.2} {:.4} {:.4})">{}</text>"##,
            lx,
            -ly,
            colors[b.idx],
            LABEL_FS,
            anchor,
            -rot,
            lx,
            -ly,
            xesc(b.name)
        );
    }

    // testament side labels (matplotlib rotation 90 CCW / -90 -> svg -90 / +90)
    let _ = write!(
        s,
        r##"<text x="{:.4}" y="0" fill="#444444" font-size="{:.4}" font-weight="bold" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 {:.4} 0)">{}</text>"##,
        -(r + 0.13),
        TNT_FS,
        -(r + 0.13),
        xesc(&st.ot)
    );
    let _ = write!(
        s,
        r##"<text x="{:.4}" y="0" fill="#444444" font-size="{:.4}" font-weight="bold" text-anchor="middle" dominant-baseline="middle" transform="rotate(90 {:.4} 0)">{}</text>"##,
        r + 0.13,
        TNT_FS,
        r + 0.13,
        xesc(&st.nt)
    );

    // title + subtitle (top centre)
    let _ = write!(
        s,
        r##"<text x="0" y="{:.4}" fill="#222222" font-size="{:.4}" font-weight="bold" text-anchor="middle" dominant-baseline="hanging">{}</text>"##,
        -(l - 0.05),
        TITLE_FS,
        xesc(&st.title)
    );
    let _ = write!(
        s,
        r##"<text x="0" y="{:.4}" fill="#666666" font-size="{:.4}" text-anchor="middle" dominant-baseline="hanging">{}</text>"##,
        -(l - 0.13),
        SUB_FS,
        xesc(&st.subtitle)
    );
}

// ---------------------------------------------------------------------------
// Rasterization
// ---------------------------------------------------------------------------
fn svg_to_pixmap(svg: &str, scale: f64, opt: &usvg::Options) -> Result<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_data(svg.as_bytes(), opt).context("parsing generated SVG")?;
    let size = tree
        .size()
        .to_int_size()
        .scale_by(scale as f32)
        .context("scaled to zero size")?;
    let mut pixmap =
        tiny_skia::Pixmap::new(size.width(), size.height()).context("allocating pixmap")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale as f32, scale as f32),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// usvg options backed by the host's installed fonts (no font is bundled). The
/// ring/title `<text>` carries no `font-family`, so usvg falls back to
/// `font_family` — set here to the first common sans-serif that is installed.
fn text_options() -> usvg::Options<'static> {
    let mut opt = usvg::Options::default();
    let db = opt.fontdb_mut();
    db.load_system_fonts();
    opt.font_family = pick_sans_family(db);
    opt
}

fn pick_sans_family(db: &fontdb::Database) -> String {
    const CANDIDATES: [&str; 7] = [
        "Arial",
        "Helvetica",
        "Liberation Sans",
        "DejaVu Sans",
        "Segoe UI",
        "Noto Sans",
        "Roboto",
    ];
    for cand in CANDIDATES {
        let present = db.faces().any(|f| {
            f.families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(cand))
        });
        if present {
            return cand.to_string();
        }
    }
    // fall back to any installed family (or a generic name if none at all)
    db.faces()
        .next()
        .and_then(|f| f.families.first().map(|(n, _)| n.clone()))
        .unwrap_or_else(|| "sans-serif".to_string())
}

/// Crop away the surrounding white border (approximates matplotlib
/// `bbox_inches="tight"`), keeping `pad` pixels of margin.
fn crop_nonwhite(pm: &tiny_skia::Pixmap, pad: u32) -> tiny_skia::Pixmap {
    let (w, h) = (pm.width(), pm.height());
    let px = pm.pixels();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    let mut found = false;
    for y in 0..h {
        let row = (y * w) as usize;
        for x in 0..w {
            let p = px[row + x as usize];
            if p.red() != 255 || p.green() != 255 || p.blue() != 255 {
                found = true;
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if !found {
        return pm.clone();
    }
    let x0 = x0.saturating_sub(pad);
    let y0 = y0.saturating_sub(pad);
    let x1 = (x1 + pad).min(w - 1);
    let y1 = (y1 + pad).min(h - 1);
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut out = tiny_skia::Pixmap::new(cw, ch).expect("non-zero crop size");
    out.fill(tiny_skia::Color::WHITE);
    out.draw_pixmap(
        -(x0 as i32),
        -(y0 as i32),
        pm.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    out
}

// ---------------------------------------------------------------------------
// Static SVG + PNG
// ---------------------------------------------------------------------------
pub fn render_static(
    bible: &Bible,
    svg_path: &Path,
    png_path: &Path,
    write_svg: bool,
    write_png: bool,
) -> Result<()> {
    println!("[4/7] Rendering static graphic ...");
    let svg = build_svg(bible, true);
    if write_svg {
        std::fs::write(svg_path, &svg)
            .with_context(|| format!("writing {}", svg_path.display()))?;
        println!("      wrote {}", svg_path.display());
    }
    if write_png {
        let pm = svg_to_pixmap(&svg, 1.0, &text_options())?;
        pm.save_png(png_path)
            .with_context(|| format!("writing {}", png_path.display()))?;
        println!(
            "      wrote {}  ({} dpi)",
            png_path.display(),
            cfg::DPI as i64
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Small README thumbnail
// ---------------------------------------------------------------------------
pub fn render_png_sm(bible: &Bible, png_path: &Path) -> Result<()> {
    println!("[5/7] Rendering small README thumbnail ...");
    let svg = build_svg(bible, true);
    let opt = text_options();
    let budget = cfg::PNG_SM_MAX_BYTES;
    let mut fallback: Option<(f64, Vec<u8>)> = None;
    for &dpi in &cfg::PNG_SM_DPIS {
        let scale = dpi / cfg::DPI;
        let pm = svg_to_pixmap(&svg, scale, &opt)?;
        let cropped = crop_nonwhite(&pm, (0.10 * dpi).round() as u32);
        let bytes = cropped.encode_png().context("encoding thumbnail PNG")?;
        if bytes.len() <= budget {
            std::fs::write(png_path, &bytes)?;
            println!(
                "      wrote {}  ({} dpi, {} KB)",
                png_path.display(),
                dpi as i64,
                bytes.len() / 1024
            );
            return Ok(());
        }
        fallback = Some((dpi, bytes));
    }
    if let Some((dpi, bytes)) = fallback {
        std::fs::write(png_path, &bytes)?;
        println!(
            "      wrote {}  ({} dpi, {} KB — still above {} KB target)",
            png_path.display(),
            dpi as i64,
            bytes.len() / 1024,
            budget / 1024
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Interactive HTML
// ---------------------------------------------------------------------------
pub fn render_html(bible: &Bible, html_path: &Path) -> Result<()> {
    println!("[6/7] Building interactive HTML ...");
    let l = cfg::L;
    let st = bible.lang.strings();
    let colors: Vec<String> = bible
        .books
        .iter()
        .map(|b| hex(book_hue_rgb(b.hue)))
        .collect();

    // clean, label-free background for crisp overlay labels (no text -> no fonts)
    let bg_svg = build_svg(bible, false);
    let bg_png = svg_to_pixmap(&bg_svg, 200.0 / cfg::DPI, &usvg::Options::default())?
        .encode_png()
        .context("encoding HTML background PNG")?;
    let bg_b64 = base64::engine::general_purpose::STANDARD.encode(&bg_png);

    // interactive Old<->New chords with tooltips
    let mut chords = String::new();
    let mut n_chords = 0usize;
    for e in bible.edges.iter().filter(|e| e.cross) {
        let (x1, y1) = bible.verse_xy(e.src, cfg::R);
        let (x2, y2) = bible.verse_xy(e.tgt, cfg::R);
        let (cx, cy) = chord_control(x1, y1, x2, y2);
        let tip = format!(
            "{} → {}",
            bible.verse_label(e.src),
            bible.verse_label(e.tgt)
        );
        let _ = write!(
            chords,
            r##"<path class="c" d="M{:.4},{:.4} Q{:.4},{:.4} {:.4},{:.4}" stroke="{}" fill="none" stroke-width="0.004" stroke-opacity="0.7"><title>{}</title></path>"##,
            x1,
            -y1,
            cx,
            -cy,
            x2,
            -y2,
            colors[e.sb],
            xesc(&tip)
        );
        n_chords += 1;
    }
    println!("      {n_chords} interactive Old<->New chords");

    // crisp vector book labels
    let mut labels = String::new();
    for b in &bible.books {
        let a = b.mid.to_radians();
        let lx = (cfg::R + 0.04) * a.cos();
        let ly = (cfg::R + 0.04) * a.sin();
        let deg = b.mid.rem_euclid(360.0);
        let (rot, anchor) = if deg > 90.0 && deg < 270.0 {
            (deg - 180.0, "end")
        } else {
            (deg, "start")
        };
        let _ = write!(
            labels,
            r##"<text x="{:.4}" y="{:.4}" fill="{}" font-size="0.026" font-weight="bold" text-anchor="{}" dominant-baseline="middle" transform="rotate({:.2} {:.4} {:.4})">{}</text>"##,
            lx,
            -ly,
            colors[b.idx],
            anchor,
            -rot,
            lx,
            -ly,
            xesc(b.name)
        );
    }

    let img = format!(
        r##"<image id="bg" x="{:.4}" y="{:.4}" width="{:.4}" height="{:.4}" href="data:image/png;base64,{}" preserveAspectRatio="none"/>"##,
        -l,
        -l,
        2.0 * l,
        2.0 * l,
        bg_b64
    );
    let vb = format!("{:.3} {:.3} {:.3} {:.3}", -l, -l, 2.0 * l, 2.0 * l);
    let vb0 = format!("{:.3}, {:.3}, {:.3}, {:.3}", -l, -l, 2.0 * l, 2.0 * l);

    let html = HTML_TEMPLATE
        .replace("%%LANG%%", &st.html_lang)
        .replace("%%TITLE%%", &xesc(&st.title))
        .replace("%%SUBTITLE%%", &xesc(&st.subtitle))
        .replace("%%TOGGLE%%", &xesc(&st.toggle))
        .replace("%%RESET%%", &xesc(&st.reset))
        .replace("%%HINT%%", &xesc(&st.hint))
        .replace("%%VB%%", &vb)
        .replace("%%VB0%%", &vb0)
        .replace("%%IMG%%", &img)
        .replace("%%CHORDS%%", &chords)
        .replace("%%LABELS%%", &labels);

    std::fs::write(html_path, &html).with_context(|| format!("writing {}", html_path.display()))?;
    println!(
        "      wrote {}  ({:.1} MB)",
        html_path.display(),
        html.len() as f64 / 1e6
    );
    Ok(())
}

const HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="%%LANG%%">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>%%TITLE%%</title>
<style>
  :root { color-scheme: light; }
  body { margin:0; font-family: system-ui, "Segoe UI", Arial, sans-serif;
         background:#fafafa; color:#222; }
  header { padding:14px 18px 6px; }
  h1 { font-size:18px; margin:0; }
  p.sub { font-size:12px; color:#666; margin:4px 0 0; }
  #toolbar { padding:8px 18px; font-size:13px; display:flex; gap:18px;
             align-items:center; flex-wrap:wrap; }
  #wrap { width:100vw; height:calc(100vh - 110px); overflow:hidden;
           background:#fff; border-top:1px solid #eee; cursor:grab; }
  #wrap.drag { cursor:grabbing; }
  svg { width:100%; height:100%; display:block; touch-action:none; }
  .c:hover { stroke-width:0.012 !important; stroke-opacity:1 !important; }
  button { font-size:13px; padding:3px 10px; cursor:pointer; }
  .hint { color:#888; }
</style>
</head>
<body>
<header>
  <h1>%%TITLE%%</h1>
  <p class="sub">%%SUBTITLE%%</p>
</header>
<div id="toolbar">
  <label><input type="checkbox" id="otnt"> %%TOGGLE%%</label>
  <button id="reset">%%RESET%%</button>
  <span class="hint">%%HINT%%</span>
</div>
<div id="wrap">
  <svg id="svg" viewBox="%%VB%%" preserveAspectRatio="xMidYMid meet">
    <g id="bglayer">%%IMG%%</g>
    <g id="chords">
      %%CHORDS%%
    </g>
    <g id="labels">
      %%LABELS%%
    </g>
  </svg>
</div>
<script>
  const svg = document.getElementById('svg');
  const wrap = document.getElementById('wrap');
  const VB0 = [%%VB0%%];
  let vb = VB0.slice();
  function apply() { svg.setAttribute('viewBox', vb.join(' ')); }

  wrap.addEventListener('wheel', (e) => {
    e.preventDefault();
    const r = svg.getBoundingClientRect();
    const mx = vb[0] + (e.clientX - r.left)/r.width  * vb[2];
    const my = vb[1] + (e.clientY - r.top )/r.height * vb[3];
    const k = Math.exp((e.deltaY||0) * 0.0015);
    vb[2]*=k; vb[3]*=k;
    vb[0] = mx - (e.clientX - r.left)/r.width  * vb[2];
    vb[1] = my - (e.clientY - r.top )/r.height * vb[3];
    apply();
  }, {passive:false});

  let drag=false, px=0, py=0;
  wrap.addEventListener('pointerdown', (e)=>{drag=true;px=e.clientX;py=e.clientY;
       wrap.classList.add('drag'); wrap.setPointerCapture(e.pointerId);});
  wrap.addEventListener('pointermove', (e)=>{ if(!drag) return;
       const r=svg.getBoundingClientRect();
       vb[0]-=(e.clientX-px)/r.width *vb[2];
       vb[1]-=(e.clientY-py)/r.height*vb[3];
       px=e.clientX; py=e.clientY; apply(); });
  function endDrag(){drag=false; wrap.classList.remove('drag');}
  wrap.addEventListener('pointerup', endDrag);
  wrap.addEventListener('pointercancel', endDrag);

  document.getElementById('reset').onclick=()=>{vb=VB0.slice();apply();};
  document.getElementById('otnt').onchange=(e)=>{
    document.getElementById('bglayer').style.display = e.target.checked ? 'none':'';
  };
  apply();
</script>
</body>
</html>
"#;
