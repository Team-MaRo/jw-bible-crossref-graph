# Bibel-Querverweise — Circular Bible Cross-Reference Graphic

Turns a JW Library **Study Bible** publication (`nwtsty_X.jwpub`) into a circular diagram of all
its cross-references: the 66 Bible books arranged around a ring, every marginal cross-reference
drawn as a chord through the circle, coloured by source book, with **Old↔New Testament links
highlighted**. Inspired by the "All Prophecies about Jesus" arc diagram, bent into a circle.

It is a single script — re-download a Bible update and rerun it to regenerate everything.

[![License](https://img.shields.io/github/license/Team-MaRo/jw-bible-crossref-graph)](LICENSE.txt)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.0-4baaaa)][code-of-conduct]

![preview](bible_circle.png)

## What you get

Running the script produces these files (next to the input `.jwpub`):

| File | Description |
| --- | --- |
| `bible_circle.png` | High-resolution raster image (300 dpi) |
| `bible_circle.svg` | Vector image (print quality; large file) |
| `bible_circle.html` | **Interactive** viewer — zoom, pan, hover a line to see the two verses, toggle "Old↔New only". Self-contained, works offline. |
| `bible_crossrefs.csv` | Every cross-reference as readable verse refs (e.g. `Jes 7:14 → Mat 1:23`) |
| `nwtsty_X.db` | The Bible's SQLite database, extracted from the `.jwpub` |

## Prerequisites

- **Python 3.8+** (developed/tested on 3.14)
- **matplotlib** — the only third-party library:
  ```bash
  pip install -r requirements.txt   # or: pip install matplotlib
  ```
- A **Study Bible `.jwpub` file** (see below).

### Don't want Python on your host?

This repo ships a [dev container](.devcontainer/devcontainer.json). Open the folder in
**VS Code** and choose **"Reopen in Container"** (Dev Containers extension), or open it in
**GitHub Codespaces**. The container is a Python 3.14 image with `matplotlib` installed
automatically — drop your `.jwpub` into the folder and jump straight to [Step 2](#step-2--run-the-script).

## Step 1 — Get the Bible file

The graphic is built from the **New World Translation Study Bible** publication file,
`nwtsty_X.jwpub`. `nwtsty` is its symbol; the `_X` marks the encrypted/packaged variant.

How to obtain it:

1. Go to **<https://www.jw.org/de/bibliothek/bibel/>** (jw.org → Bibliothek → Bibel).
2. Open the **Studienausgabe** (Study Bible) and use the **download / Herunterladen** option,
   choosing the **JWPUB** format. Only the *study* edition contains the cross-references this
   script needs — the regular reading edition does not.
3. Save the downloaded `nwtsty_X.jwpub` into this folder, next to `bible_circle.py`.

(Alternatively, the free **JW Library** desktop app stores downloaded publications as `.jwpub`
files in its app-data folder, where the study Bible is likewise named `nwtsty_X.jwpub`.)

> Any future `nwtsty` `.jwpub` update works — and the script also accepts any other NWT `.jwpub`
> edition that includes cross-references. The regular (non-study) Bible, and the EPUB/PDF/RTF
> downloads, do **not** contain the cross-reference data and will not work.

No password or decryption is required: the `.jwpub` is just a zip-within-a-zip around a plain
SQLite database, which the script reads directly.

## Step 2 — Run the script

From this folder:

```bash
# point it at the file explicitly
python bible_circle.py nwtsty_X.jwpub

# ...or let it auto-find the first *.jwpub in this folder
python bible_circle.py
```

You'll see progress for each stage (extracting the database, loading ~65,600 cross-references,
writing the CSV, rendering the SVG/PNG, building the HTML). When it finishes, open
`bible_circle.png` or, for the interactive version, `bible_circle.html` in a web browser.

## Step 3 — Use the interactive viewer

Open **`bible_circle.html`** in any modern browser:

- **scroll** to zoom, **drag** to pan
- **hover** any coloured line to see the two verses it links
- tick **"Nur Verweise zwischen Altem und Neuem Testament zeigen"** to hide the dense background
  and show only the Old↔New Testament connections

## Customising the look

All settings live in the `CFG` dictionary at the top of `bible_circle.py` — gap sizes between
books, the Gospel-bracket spacing, chord curvature, line opacity/width, colours, image size/DPI,
title text, and which outputs to produce. Edit and rerun. See [AGENTS.md](AGENTS.md) for details.

## Notes

- Output is in **German** (this is the German Studienbibel): full German book names around the
  ring, German titles, German verse abbreviations in the CSV.
- The themes and line styles of the original "All Prophecies about Jesus" infographic were
  hand-curated by its author and are **not** part of the Bible data, so this graphic colours by
  source book instead — the encoding the data actually supports.

## Contributing

Please read [CONTRIBUTING.md][contributing] for details on our code of conduct and the process for submitting pull requests.

This project uses [Conventional Commits](https://www.conventionalcommits.org/).

## Versioning

This project is **not versioned** — there are no releases, tags, or version numbers. It's a single
self-contained script that you re-run against the latest Bible `.jwpub` to regenerate the output.
Just use the latest state of `master`.

## Authors

### Special thanks for all the people who had helped this project so far

- **Manuele** - [D3strukt0r](https://github.com/D3strukt0r)

See also the full list of [contributors][gh-contributors] who participated in this project.

### I would like to join this list. How can I help the project?

We're currently looking for contributions for the following:

- [ ] Bug fixes
- [ ] Translations
- [ ] etc...

For more information, please refer to our [CONTRIBUTING.md][contributing] guide.

## License

This project is licensed under the MIT License - see the [LICENSE.txt](LICENSE.txt) file for details.

## Acknowledgments

This project currently uses no third-party libraries or copied code.

[gh-contributors]: https://github.com/Team-MaRo/jw-bible-crossref-graph/contributors
[contributing]: https://github.com/Team-MaRo/.github/blob/master/CONTRIBUTING.md
[code-of-conduct]: https://github.com/Team-MaRo/.github/blob/master/CODE_OF_CONDUCT.md
