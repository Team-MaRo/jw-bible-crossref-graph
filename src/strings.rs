//! Localisation. The `.jwpub` supplies only verse ids; every book name and all
//! UI text is applied from here, so `--lang` only relabels the graphic.
//!
//! The actual strings live in `i18n/<lang>.yaml`, embedded at build time and
//! parsed once on first use. To add a language: add `i18n/<x>.yaml`, a `Lang`
//! variant, and a `static` below.

use serde::Deserialize;
use std::sync::LazyLock;

/// Books 1..=39 are the Old Testament (language-independent).
pub const N_OT_BOOKS: i64 = 39;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::De => &DE,
            Lang::En => &EN,
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }
}

#[derive(Deserialize)]
pub struct Strings {
    pub html_lang: String,
    pub title: String,
    pub subtitle: String,
    pub ot: String,
    pub nt: String,
    pub toggle: String,
    pub reset: String,
    pub hint: String,
    pub csv_header: Vec<String>,
    pub abbr: Vec<String>,
    pub name: Vec<String>,
}

static DE: LazyLock<Strings> = LazyLock::new(|| parse("de", include_str!("../i18n/de.yaml")));
static EN: LazyLock<Strings> = LazyLock::new(|| parse("en", include_str!("../i18n/en.yaml")));

/// Parse an embedded i18n file and validate its shape. The YAML is bundled into
/// the binary, so a malformed file is a build/authoring error — fail loudly.
fn parse(lang: &str, src: &str) -> Strings {
    let s: Strings = serde_yaml_ng::from_str(src)
        .unwrap_or_else(|e| panic!("i18n/{lang}.yaml: invalid YAML: {e}"));
    assert_eq!(
        s.abbr.len(),
        66,
        "i18n/{lang}.yaml: `abbr` must list 66 books"
    );
    assert_eq!(
        s.name.len(),
        66,
        "i18n/{lang}.yaml: `name` must list 66 books"
    );
    assert_eq!(
        s.csv_header.len(),
        6,
        "i18n/{lang}.yaml: `csv_header` must have 6 columns"
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_languages_parse_and_validate() {
        // forces the LazyLock parse + length asserts for every language
        for lang in [Lang::De, Lang::En] {
            let s = lang.strings();
            assert_eq!(s.abbr.len(), 66);
            assert_eq!(s.name.len(), 66);
            assert!(!s.title.is_empty());
        }
        // spot-check a few values, incl. "Off" (must stay a string, not YAML bool)
        assert_eq!(Lang::De.strings().abbr[65], "Off");
        assert_eq!(Lang::De.strings().name[0], "1. Mose");
        assert_eq!(Lang::En.strings().abbr[22], "Isa");
        assert_eq!(Lang::En.strings().name[65], "Revelation");
    }
}
