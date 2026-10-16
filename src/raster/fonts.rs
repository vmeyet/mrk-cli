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
/// Styles file names add after the family (`Arial Bold Italic.ttf`), longest first so `semibold` is not read as `bold`.
const STYLE_SUFFIXES: [&str; 11] =
    ["extrabold", "semibold", "bold", "italic", "oblique", "regular", "medium", "light", "thin", "black", "heavy"];
const MAX_DIRECTORY_DEPTH: usize = 4;
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];
/// Above Latin Extended and IPA, labels need the scripts only the fallback or full system set covers.
const LAST_LATIN_CODE_POINT: u32 = 0x02ff;
/// The family of `LAST_RESORT_FONT`, renamed from DejaVu Sans so it never shadows a system font the SVG lists.
const LAST_RESORT_FAMILY: &str = "mrk Sans";
/// A Latin subset of DejaVu Sans (`assets/fonts/LICENSE-DejaVu`, rebuilt by `scripts/last-resort-font`), so labels render on a system without fonts.
static LAST_RESORT_FONT: &[u8] = include_bytes!("../../assets/fonts/MrkSans.ttf");

/// The font directories are walked once, whichever sets load from them.
static FONT_FILES: LazyLock<Vec<PathBuf>> =
    LazyLock::new(|| font_directories().iter().flat_map(|directory| font_files(directory)).collect());
static LABEL_FONTS: LazyLock<Arc<Database>> = LazyLock::new(|| Arc::new(fonts_named(&FONT_FILES, &FAMILY_STEMS)));
static FALLBACK_FONTS: LazyLock<Arc<Database>> =
    LazyLock::new(|| Arc::new(fonts_named(&FONT_FILES, &[FAMILY_STEMS.as_slice(), &FALLBACK_STEMS].concat())));
static SYSTEM_FONTS: LazyLock<Arc<Database>> = LazyLock::new(|| Arc::new(fonts_from(FONT_FILES.iter())));

#[derive(Debug, PartialEq, Eq)]
enum FontSet {
    Label,
    Fallback,
    System,
}

/// The fonts a diagram needs: the label families for Latin, which the last resort covers when none is installed, the
/// listed fallbacks when they cover every other glyph, or every font.
pub fn for_text(text: &str) -> Arc<Database> {
    let fonts = match font_set(text, || &FALLBACK_FONTS) {
        FontSet::Label => &LABEL_FONTS,
        FontSet::Fallback => &FALLBACK_FONTS,
        FontSet::System => &SYSTEM_FONTS,
    };
    Arc::clone(fonts)
}

/// `fallback` is only called, and its fonts only loaded, when the text goes beyond Latin.
fn font_set<'a>(text: &str, fallback: impl FnOnce() -> &'a Database) -> FontSet {
    if is_latin(text) {
        FontSet::Label
    } else if covers(fallback(), text) {
        FontSet::Fallback
    } else {
        FontSet::System
    }
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

fn fonts_named(files: &[PathBuf], stems: &[&str]) -> Database {
    fonts_from(files.iter().filter(|path| stems.contains(&family_stem(path).as_str())))
}

/// usvg ends every font-family list on the generic `serif`, so pointing it at the embedded face makes that face the last match.
fn fonts_from<'a>(files: impl Iterator<Item = &'a PathBuf>) -> Database {
    let mut database = Database::new();
    files.for_each(|path| {
        let _ = database.load_font_file(path);
    });
    database.load_font_source(Source::Binary(Arc::new(LAST_RESORT_FONT)));
    database.set_serif_family(LAST_RESORT_FAMILY);
    database
}

fn font_directories() -> Vec<PathBuf> {
    let home = std::env::home_dir().unwrap_or_default();
    let system = ["/System/Library/Fonts", "/Library/Fonts", "/usr/share/fonts", "/usr/local/share/fonts"].map(PathBuf::from);
    let user = ["Library/Fonts", ".local/share/fonts", ".fonts"].map(|relative| home.join(relative));
    system.into_iter().chain(user).collect()
}

fn font_files(directory: &Path) -> Vec<PathBuf> {
    files_under(directory, MAX_DIRECTORY_DEPTH).into_iter().filter(|path| is_font_file(path)).collect()
}

/// Links are followed to files only (macOS ships `Arial Unicode.ttf` as one), never to directories, so a link loop
/// cannot keep the walk going.
fn files_under(directory: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else { return Vec::new() };
    entries
        .flatten()
        .flat_map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() && depth > 0 => files_under(&entry.path(), depth - 1),
            Ok(kind) if kind.is_file() || (kind.is_symlink() && entry.path().is_file()) => vec![entry.path()],
            _ => Vec::new(),
        })
        .collect()
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| FONT_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
}

/// `NotoSansArabic[wdth,wght].ttf`, `InterVariable.ttf` and `Arial Bold Italic.ttf` name the same families as their
/// regular files.
fn family_stem(path: &Path) -> String {
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    let family = stem.split(['-', '_', '[']).next().unwrap_or_default().to_ascii_lowercase().replace(' ', "");
    without_styles(family.strip_suffix("variable").unwrap_or(&family)).to_owned()
}

fn without_styles(family: &str) -> &str {
    STYLE_SUFFIXES.iter().find_map(|style| family.strip_suffix(style).filter(|rest| !rest.is_empty())).map_or(family, without_styles)
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
        assert!(is_label_family("/System/Library/Fonts/Supplemental/Arial Bold.ttf"));
        assert!(is_label_family("/System/Library/Fonts/Supplemental/Arial Bold Italic.ttf"));
        assert!(is_label_family("/Library/Fonts/Inter SemiBold.otf"));
        assert!(!is_label_family("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"));
        assert!(!is_label_family("/System/Library/Fonts/NotoSansArmenian.ttc"));
        assert!(!is_label_family("/Library/Fonts/Bold.ttf"));
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
        let database = fonts_from(std::iter::empty());
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
        assert!(has_label(fonts_from(std::iter::empty())));
    }

    /// Font files in a folder of their own, each a copy of the embedded face.
    fn font_folder(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
        let folder = tempfile::tempdir().unwrap();
        let files = names.iter().map(|name| folder.path().join(name)).collect::<Vec<_>>();
        for path in &files {
            std::fs::write(path, LAST_RESORT_FONT).unwrap();
        }
        (folder, files)
    }

    #[test]
    fn the_label_fonts_load_the_named_families_and_the_last_resort_only() {
        let (_folder, files) = font_folder(&["Arial Bold.ttf", "Courier.ttf", "Menlo.ttc"]);

        assert_eq!(fonts_named(&files, &FAMILY_STEMS).len(), 2);
        assert_eq!(fonts_named(&files, &["menlo"]).len(), 2);
        assert_eq!(fonts_from(files.iter()).len(), 4);
    }

    #[test]
    fn without_a_label_family_installed_only_the_last_resort_loads() {
        let (_folder, files) = font_folder(&["Courier.ttf", "Menlo.ttc"]);

        let database = fonts_named(&files, &FAMILY_STEMS);

        assert_eq!(database.len(), 1);
        assert!(covers(&database, "Start"));
    }

    #[test]
    fn only_text_beyond_latin_asks_for_the_fallbacks_and_their_misses_for_every_font() {
        let fallback = fonts_from(std::iter::empty());

        assert_eq!(font_set("Café", || unreachable!("fallbacks loaded for Latin")), FontSet::Label);
        assert_eq!(font_set("Start → End", || &fallback), FontSet::Fallback);
        assert_eq!(font_set("開始", || &fallback), FontSet::System);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_followed_to_files_but_not_to_directories() {
        let root = tempfile::tempdir().unwrap();
        let fonts = root.path().join("fonts");
        std::fs::create_dir(&fonts).unwrap();
        std::fs::write(root.path().join("Real.ttf"), b"").unwrap();
        std::os::unix::fs::symlink(root.path().join("Real.ttf"), fonts.join("Linked.ttf")).unwrap();
        std::os::unix::fs::symlink(&fonts, fonts.join("loop")).unwrap();
        std::fs::write(fonts.join("fonts.dir"), b"").unwrap();

        assert_eq!(font_files(&fonts), [fonts.join("Linked.ttf")]);
    }

    #[test]
    fn only_font_files_are_read() {
        assert!(is_font_file(Path::new("/usr/share/fonts/DejaVuSans.ttf")));
        assert!(is_font_file(Path::new("/System/Library/Fonts/Hiragino Sans GB.TTC")));
        assert!(!is_font_file(Path::new("/usr/share/fonts/fonts.dir")));
        assert!(!is_font_file(Path::new("/Library/Fonts/README")));
    }

    #[test]
    #[ignore = "needs CJK and Arabic fonts installed; run with --ignored on a desktop system"]
    fn text_beyond_latin_finds_a_face_for_every_glyph() {
        assert!(covers(&for_text("開始 → مرحبا"), "開始 → مرحبا"));
    }
}
