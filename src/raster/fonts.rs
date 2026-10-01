use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use resvg::usvg::fontdb::{Database, ID, Source};

/// The families text is drawn in, the CSS `font-family` value of every SVG mrk draws.
pub const FONT_FAMILY: &str =
    "Inter, \"Helvetica Neue\", Helvetica, \"DejaVu Sans\", \"Noto Sans\", \"Liberation Sans\", Arial, sans-serif";
/// File-name stems of the families in `FONT_FAMILY`, lowercased without separators.
const FAMILY_STEMS: [&str; 7] = ["inter", "helveticaneue", "helvetica", "dejavusans", "notosans", "liberationsans", "arial"];
/// File-name stems of macOS and Linux fonts for the common scripts beyond Latin, so a label in them skips loading every system font.
const FALLBACK_STEMS: [&str; 15] = [
    "pingfang",
    "hiraginosansgb",
    "applesdgothicneo",
    "notosanscjk",
    "geezapro",
    "sfarabic",
    "notosansarabic",
    "sfhebrew",
    "arialhb",
    "notosanshebrew",
    "kohinoor",
    "devanagarimt",
    "notosansdevanagari",
    "thonburi",
    "notosansthai",
];
const MAX_DIRECTORY_DEPTH: usize = 4;
/// Above Latin Extended and IPA, labels need the scripts only the fallback or full system set covers.
const LAST_LATIN_CODE_POINT: u32 = 0x02ff;
/// The family of `LAST_RESORT_FONT`, renamed from DejaVu Sans so it never shadows a system font the SVG lists.
const LAST_RESORT_FAMILY: &str = "mrk Sans";
/// A Latin subset of DejaVu Sans (`assets/fonts/LICENSE-DejaVu`, rebuilt by `scripts/last-resort-font`), so labels render on a system without fonts.
static LAST_RESORT_FONT: &[u8] = include_bytes!("../../assets/fonts/MrkSans.ttf");

static LABEL_FONTS: LazyLock<Arc<Database>> = LazyLock::new(|| Arc::new(with_last_resort(label_fonts())));
static FALLBACK_FONTS: LazyLock<Arc<Database>> =
    LazyLock::new(|| Arc::new(with_last_resort(fonts_named(&[FAMILY_STEMS.as_slice(), &FALLBACK_STEMS].concat()))));
static SYSTEM_FONTS: LazyLock<Arc<Database>> = LazyLock::new(|| Arc::new(with_last_resort(system_fonts())));

/// The fonts a diagram needs: the label families alone, the listed fallbacks when they cover every glyph, or the whole system set.
pub fn for_text(text: &str) -> Arc<Database> {
    let fonts = if is_latin(text) {
        &LABEL_FONTS
    } else if covers(&FALLBACK_FONTS, text) {
        &FALLBACK_FONTS
    } else {
        &SYSTEM_FONTS
    };
    Arc::clone(fonts)
}

fn is_latin(text: &str) -> bool {
    text.chars().all(is_latin_character)
}

fn is_latin_character(character: char) -> bool {
    u32::from(character) <= LAST_LATIN_CODE_POINT
}

fn covers(database: &Database, text: &str) -> bool {
    let wanted: BTreeSet<char> = text.chars().filter(|character| !is_latin_character(*character)).collect();
    let found: BTreeSet<char> = database.faces().flat_map(|face| glyphs_among(database, face.id, &wanted)).collect();
    found.len() == wanted.len()
}

fn glyphs_among(database: &Database, face: ID, characters: &BTreeSet<char>) -> Vec<char> {
    database
        .with_face_data(face, |data, index| {
            let Ok(face) = ttf_parser::Face::parse(data, index) else { return Vec::new() };
            characters.iter().copied().filter(|character| face.glyph_index(*character).is_some()).collect()
        })
        .unwrap_or_default()
}

fn system_fonts() -> Database {
    let mut database = Database::new();
    database.load_system_fonts();
    database
}

fn label_fonts() -> Database {
    let database = fonts_named(&FAMILY_STEMS);
    if database.is_empty() { system_fonts() } else { database }
}

/// usvg ends every font-family list on the generic `serif`, so pointing it at the embedded face makes that face the last match.
fn with_last_resort(mut database: Database) -> Database {
    database.load_font_source(Source::Binary(Arc::new(LAST_RESORT_FONT)));
    database.set_serif_family(LAST_RESORT_FAMILY);
    database
}

fn fonts_named(stems: &[&str]) -> Database {
    let mut database = Database::new();
    font_directories()
        .iter()
        .flat_map(|directory| font_files(directory, MAX_DIRECTORY_DEPTH))
        .filter(|path| stems.contains(&family_stem(path).as_str()))
        .for_each(|path| {
            let _ = database.load_font_file(path);
        });
    database
}

fn font_directories() -> Vec<PathBuf> {
    let home = std::env::home_dir().unwrap_or_default();
    let system = ["/System/Library/Fonts", "/Library/Fonts", "/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from);
    let user = ["Library/Fonts", ".local/share/fonts", ".fonts"].map(|relative| home.join(relative));
    system.into_iter().chain(user).collect()
}

fn font_files(directory: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    entries
        .flatten()
        .flat_map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() && depth > 0 => font_files(&entry.path(), depth - 1),
            Ok(kind) if kind.is_file() => vec![entry.path()],
            _ => Vec::new(),
        })
        .collect()
}

/// `NotoSansArabic[wdth,wght].ttf` and `InterVariable.ttf` name the same families as their static files.
fn family_stem(path: &Path) -> String {
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    let family = stem.split(['-', '_', '[']).next().unwrap_or_default().to_ascii_lowercase().replace(' ', "");
    family.strip_suffix("variable").map(str::to_owned).unwrap_or(family)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use resvg::usvg::fontdb::{Family, Query};

    fn is_label_family(path: &str) -> bool {
        FAMILY_STEMS.contains(&family_stem(Path::new(path)).as_str())
    }

    fn is_fallback(path: &str) -> bool {
        FALLBACK_STEMS.contains(&family_stem(Path::new(path)).as_str())
    }

    #[test]
    fn label_family_matches_regular_and_styled_files_only() {
        assert!(is_label_family("/System/Library/Fonts/HelveticaNeue.ttc"));
        assert!(is_label_family("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"));
        assert!(is_label_family("/Users/me/Library/Fonts/InterVariable.ttf"));
        assert!(!is_label_family("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"));
        assert!(!is_label_family("/System/Library/Fonts/NotoSansArmenian.ttc"));
    }

    #[test]
    fn fallback_matches_macos_linux_and_variable_file_names() {
        assert!(is_fallback("/System/Library/Fonts/Hiragino Sans GB.ttc"));
        assert!(is_fallback("/System/Library/Fonts/Supplemental/Thonburi.ttc"));
        assert!(is_fallback("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"));
        assert!(is_fallback("/usr/share/fonts/google-noto-vf/NotoSansArabic[wdth,wght].ttf"));
        assert!(!is_fallback("/System/Library/Fonts/NotoSansArmenian.ttc"));
    }

    #[test]
    fn scripts_beyond_latin_ask_for_more_than_the_label_fonts() {
        assert!(is_latin("Café naïve"));
        assert!(!is_latin("開始"));
    }

    #[test]
    fn coverage_ignores_latin_and_needs_every_other_glyph() {
        assert!(covers(&Database::new(), "Start"));
        assert!(!covers(&Database::new(), "Start 開始"));
    }

    #[test]
    fn a_system_without_fonts_still_matches_the_label_families() {
        let database = with_last_resort(Database::new());
        let families = [Family::Name("Inter"), Family::SansSerif, Family::Serif];
        let face = database.query(&Query { families: &families, ..Query::default() }).and_then(|id| database.face(id)).unwrap();

        assert_eq!(face.families[0].0, LAST_RESORT_FAMILY);
        assert!(covers(&database, "Start → End"));
    }

    #[test]
    fn labels_draw_with_the_last_resort_alone() {
        let has_label = |database: Database| {
            let svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="20"><text y="15" font-family='{FONT_FAMILY}'>Start</text></svg>"#
            );
            crate::raster::parse_with(&svg, Arc::new(database)).unwrap().root().has_children()
        };

        assert!(!has_label(Database::new()));
        assert!(has_label(with_last_resort(Database::new())));
    }

    #[test]
    fn label_fonts_find_a_face() {
        assert!(!for_text("Start").is_empty());
    }

    #[test]
    fn text_beyond_latin_finds_a_face_for_every_glyph() {
        assert!(covers(&for_text("開始 → مرحبا"), "開始 → مرحبا"));
    }
}
