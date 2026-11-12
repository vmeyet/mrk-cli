mod budget;
mod highlight;
mod language;
mod panel;

pub(crate) use self::budget::{Allowance, highlighted_len};
use crate::document::{Line, MIN_WIDTH, Settings};

/// A fenced code block as a panel: every line exactly `settings.width` cells, at least `MIN_WIDTH`, highlighted
/// when the language is known, within the bounds of `budget.rs`. `language` is a language name (`rust`, `ts`), not a
/// whole fence info string.
pub fn render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line> {
    let alone = Allowance::new([(0, highlighted_len(code, language))]);
    render_in_document(code, language, 0, &alone, settings)
}

/// `render` for a block of a document, highlighted only when the document's `allowance` admits its `key`.
pub(crate) fn render_in_document(code: &str, language: Option<&str>, key: usize, allowance: &Allowance, settings: &Settings) -> Vec<Line> {
    panel(code, language, allowance.highlighted(key), settings)
}

/// The panel of `code`, its first `highlighted` bytes highlighted and the rest plain.
fn panel(code: &str, language: Option<&str>, highlighted: usize, settings: &Settings) -> Vec<Line> {
    let (head, tail) = code.split_at(highlighted);
    let theme = &settings.theme;
    let rows = highlight::highlight(head, language.filter(|_| !head.is_empty()), theme).chain(highlight::highlight(tail, None, theme));
    panel::panel(rows, language, settings.width.max(MIN_WIDTH), &theme.palette)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::test_settings;

    const RUST: &str = "fn main() {\n    let greeting = \"hello\";\n    println!(\"{greeting}\");\n}\n";

    fn at_width(width: usize) -> Settings {
        Settings { width, ..test_settings() }
    }

    fn framed(lines: &[Line]) -> String {
        lines.iter().map(|line| format!("│{}│\n", line.plain())).collect()
    }

    #[test]
    fn highlighting_stops_at_the_first_line_too_long_to_highlight() {
        let code = format!("let a = 1;\nlet b = \"{}\";\nlet c = 2;\n", "x".repeat(600));
        let (rust, text) = (render(&code, Some("rust"), &at_width(40)), render(&code, Some("text"), &at_width(40)));

        assert_ne!(rust[1], text[1]);
        assert_eq!(rust[2..], text[2..]);
    }

    #[test]
    fn a_block_its_document_does_not_admit_is_plain() {
        let settings = test_settings();
        let (rust, text) = (render(RUST, Some("rust"), &settings), render(RUST, Some("text"), &settings));
        let in_document = |allowance: &Allowance| render_in_document(RUST, Some("rust"), 1, allowance, &settings);

        assert_eq!(in_document(&Allowance::new([(1, RUST.len())])), rust);
        assert_eq!(in_document(&Allowance::new([(2, RUST.len())]))[1..], text[1..]);
    }

    #[test]
    fn rust_panel() {
        insta::assert_snapshot!(framed(&render(RUST, Some("rust"), &at_width(40))));
    }

    #[test]
    fn plain_panel_without_label() {
        insta::assert_snapshot!(framed(&render("just text\n\n\tindented\n", None, &at_width(24))));
    }

    #[test]
    fn long_lines_wrap_with_a_marker() {
        let code = "let numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];\n";

        insta::assert_snapshot!(framed(&render(code, Some("rs"), &at_width(30))));
    }

    #[test]
    fn a_width_under_the_minimum_renders_at_the_minimum() {
        assert!(render("let tab =\t1;\n", None, &at_width(4)).iter().all(|line| line.width() == MIN_WIDTH));
    }

    #[test]
    fn a_label_too_wide_is_shortened() {
        insta::assert_snapshot!(framed(&render("x\n", Some("averyveryverylonglanguagename"), &at_width(16))));
    }

    #[test]
    fn escape_bytes_in_code_do_not_shorten_the_background() {
        let lines = render("printf '\x1b[31mred\x1b[0m'\n", None, &at_width(24));
        let visible = |line: &Line| line.plain().chars().filter(|character| !character.is_control()).count();

        assert!(lines.iter().all(|line| visible(line) == 24), "{}", framed(&lines));
    }

    #[test]
    fn every_line_is_exactly_the_width() {
        let code = "fn wide() {\n\tlet 漢字 = \"日本語のテキストはとても長いのでここで折り返されるはずです\";\n}\n";
        for width in [MIN_WIDTH, 9, 12, 20, 40, 80, 100] {
            for language in [Some("rust"), None, Some("klingon")] {
                let lines = render(code, language, &at_width(width));
                assert!(lines.iter().all(|line| line.width() == width), "width {width}, {language:?}:\n{}", framed(&lines));
            }
        }
    }

    #[test]
    fn every_span_sits_on_the_surface() {
        let settings = test_settings();
        let lines = render(RUST, Some("rust"), &settings);

        assert!(lines.iter().flat_map(|line| &line.spans).all(|span| span.style.bg == Some(settings.theme.palette.surface)));
    }

    #[test]
    fn label_is_muted_and_continuation_is_subtle() {
        let settings = at_width(20);
        let palette = settings.theme.palette;
        let lines = render("0123456789012345678901234\n", Some("text"), &settings);

        let label = lines[0].spans.iter().find(|span| span.text == "text").map(|span| span.style.fg);
        let marker = lines[2].spans.iter().find(|span| span.text == "↪ ").map(|span| span.style.fg);
        assert_eq!(label, Some(Some(palette.muted)));
        assert_eq!(marker, Some(Some(palette.subtle)));
    }

    #[cfg(feature = "cli")]
    #[test]
    fn rust_panel_in_colour() {
        let lines = render("fn main() {\n    let a = \"b\";\n}\n", Some("rust"), &at_width(24));

        insta::assert_snapshot!(crate::snapshot::ansi(&crate::document::Document { blocks: vec![crate::document::Block::Lines(lines)] }));
    }
}
