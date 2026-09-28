mod highlight;
mod language;
mod panel;

use crate::document::{Line, Settings};

/// A fenced code block as a panel: every line exactly `settings.width` cells, highlighted when the language is known.
pub fn render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line> {
    let language = language.and_then(language::fence_token);
    let rows = highlight::highlight(code, language, &settings.theme);
    panel::panel(&rows, language, settings.width, &settings.theme.palette)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::test_settings;

    const RUST: &str = "fn main() {\n    let greeting = \"hello\";\n    println!(\"{greeting}\");\n}\n";

    fn at_width(width: usize) -> Settings {
        Settings { width, ..test_settings() }
    }

    fn framed(lines: &[Line]) -> String {
        lines.iter().map(|line| format!("│{}│\n", line.plain())).collect()
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
    fn a_label_too_wide_is_shortened() {
        insta::assert_snapshot!(framed(&render("x\n", Some("averyveryverylonglanguagename"), &at_width(16))));
    }

    #[test]
    fn every_line_is_exactly_the_width() {
        let code = "fn wide() {\n\tlet 漢字 = \"日本語のテキストはとても長いのでここで折り返されるはずです\";\n}\n";
        for width in [8, 12, 20, 40, 80, 100] {
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

    #[test]
    fn fence_info_keeps_only_the_language() {
        let lines = render("x\n", Some("rust,ignore"), &at_width(20));

        assert_eq!(lines[0].plain().trim(), "rust");
    }

    /// Run with `cargo test --release code::tests::cold -- --ignored --nocapture`, alone, so nothing warmed the sets first.
    #[test]
    #[ignore = "timing, meaningful only in release and in a fresh process"]
    fn cold_first_block_timing() {
        let start = std::time::Instant::now();
        let _ = render(RUST, Some("rust"), &test_settings());
        let rust = start.elapsed();
        let _ = render("const a: number = 1;\n", Some("ts"), &test_settings());
        println!("cold rust block: {rust:?}, then first ts block: {:?}", start.elapsed().saturating_sub(rust));
    }
}
