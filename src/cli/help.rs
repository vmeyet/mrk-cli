//! The sections after the options, shared by `--help` and the man page so the two cannot drift.
use roff::{Roff, bold, roman};

use crate::terminal::pager;

struct Section {
    title: &'static str,
    intro: &'static str,
    rows: Vec<(String, &'static str)>,
}

fn rows(pairs: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
    pairs.iter().map(|&(term, meaning)| (term.to_owned(), meaning)).collect()
}

fn sections() -> [Section; 4] {
    [
        Section {
            title: "Settings",
            intro: "Flags win over the environment, the environment over the config file.",
            rows: rows(&[
                ("MRK_THEME, MRK_WIDTH, MRK_ALIGN", "the same as --theme, --width and --align"),
                ("MRK_PAGER", "the command -p pipes into instead of the built-in pager, diagrams as text; it does not turn paging on"),
                ("NO_COLOR", "no colour while --color is auto"),
            ]),
        },
        Section {
            title: "Config file",
            intro: "The first of these that exists, a TOML file with the keys theme, theme_dark, theme_light, width, images, align, pager and jumbo_title:",
            rows: rows(&[
                ("$XDG_CONFIG_HOME/mrk/config.toml", "when XDG_CONFIG_HOME is set"),
                ("~/.config/mrk/config.toml", "otherwise"),
                ("~/Library/Application Support/mrk/config.toml", "on macOS, after the one above"),
            ]),
        },
        Section { title: "Pager keys", intro: "In the built-in pager (-p):", rows: pager::key_help() },
        Section {
            title: "Exit status",
            intro: "mrk exits with:",
            rows: rows(&[
                ("0", "success"),
                ("1", "a failure while running, such as a file that cannot be read"),
                ("2", "a usage mistake: a bad flag, environment variable or config file"),
            ]),
        },
    ]
}

fn section_text(section: &Section) -> String {
    let width = section.rows.iter().map(|(term, _)| term.chars().count()).max().unwrap_or_default();
    let rows: String = section.rows.iter().map(|(term, meaning)| format!("  {term:width$}  {meaning}\n")).collect();
    format!("{}:\n{}\n{rows}", section.title, section.intro)
}

fn section_roff(section: &Section) -> Roff {
    let mut roff = Roff::new();
    roff.control("SH", [section.title.to_uppercase().as_str()]).text([roman(section.intro)]);
    for (term, meaning) in &section.rows {
        roff.control("TP", []).text([bold(term.as_str())]).text([roman(*meaning)]);
    }
    roff
}

/// The text `--help` prints after the options.
pub fn text() -> String {
    sections().iter().map(section_text).collect::<Vec<_>>().join("\n")
}

/// The same sections as man page roff.
pub fn roff() -> String {
    sections().iter().map(|section| section_roff(section).to_roff()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_help_aligns_each_section_and_lists_the_pager_keys() {
        let text = text();

        assert!(text.contains("Pager keys:\nIn the built-in pager (-p):\n"), "{text}");
        assert!(text.contains("  q, Esc, Ctrl-c"), "{text}");
        assert!(text.contains("  0  success\n  1  a failure"), "{text}");
    }

    #[test]
    fn the_man_sections_are_headings_with_tagged_paragraphs() {
        let roff = roff();

        assert!(roff.contains(".SH \"PAGER KEYS\"\n"), "{roff}");
        assert!(roff.contains(".TP\n\\fBMRK_PAGER\\fR\n"), "{roff}");
    }
}
