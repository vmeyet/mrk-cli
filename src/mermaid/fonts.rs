use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use resvg::usvg::fontdb::Database;

/// File-name stems of the families in `svg::FONT_FAMILY`, lowercased without separators.
const FAMILY_STEMS: [&str; 7] = ["inter", "helveticaneue", "helvetica", "dejavusans", "notosans", "liberationsans", "arial"];
const MAX_DIRECTORY_DEPTH: usize = 4;
/// Above Latin Extended and IPA, labels need the scripts only the full system set covers.
const LAST_LATIN_CODE_POINT: u32 = 0x02ff;

static LABEL_FONTS: LazyLock<Arc<Database>> = LazyLock::new(|| Arc::new(label_fonts()));
static SYSTEM_FONTS: LazyLock<Arc<Database>> = LazyLock::new(|| Arc::new(system_fonts()));

/// The fonts a diagram needs: the label families alone, or the whole system set for scripts beyond Latin.
pub fn for_text(text: &str) -> Arc<Database> {
    if is_latin(text) { Arc::clone(&LABEL_FONTS) } else { Arc::clone(&SYSTEM_FONTS) }
}

fn is_latin(text: &str) -> bool {
    text.chars().all(|character| u32::from(character) <= LAST_LATIN_CODE_POINT)
}

fn system_fonts() -> Database {
    let mut database = Database::new();
    database.load_system_fonts();
    database
}

fn label_fonts() -> Database {
    let mut database = Database::new();
    font_directories()
        .iter()
        .flat_map(|directory| font_files(directory, MAX_DIRECTORY_DEPTH))
        .filter(|path| is_label_family(path))
        .for_each(|path| {
            let _ = database.load_font_file(path);
        });
    if database.is_empty() { system_fonts() } else { database }
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

fn is_label_family(path: &Path) -> bool {
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or_default();
    let family = stem.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase().replace(' ', "");
    let family = family.strip_suffix("variable").unwrap_or(&family);
    FAMILY_STEMS.contains(&family)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn label_family_matches_regular_and_styled_files_only() {
        assert!(is_label_family(Path::new("/System/Library/Fonts/HelveticaNeue.ttc")));
        assert!(is_label_family(Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")));
        assert!(is_label_family(Path::new("/Users/me/Library/Fonts/InterVariable.ttf")));
        assert!(!is_label_family(Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")));
        assert!(!is_label_family(Path::new("/System/Library/Fonts/NotoSansArmenian.ttc")));
    }

    #[test]
    fn scripts_beyond_latin_ask_for_the_system_set() {
        assert!(is_latin("Café naïve"));
        assert!(!is_latin("開始"));
    }

    #[test]
    fn label_fonts_find_a_face() {
        assert!(!for_text("Start").is_empty());
    }
}
