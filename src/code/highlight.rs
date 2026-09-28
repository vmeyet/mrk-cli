use std::sync::LazyLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, Style as SyntaxStyle};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use two_face::theme::LazyThemeSet;

use super::language;
use crate::document::{Rgb, Span, Style};
use crate::theme::Theme;

const TAB: &str = "    ";

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static THEMES: LazyLock<LazyThemeSet> = LazyLock::new(|| two_face::theme::extra().into());

/// One entry per source line: tabs expanded, no line ending, no background.
/// A language without a syntax, or a highlighting failure, gives plain text in the palette's `text`.
pub fn highlight(code: &str, language: Option<&str>, theme: &Theme) -> Vec<Vec<Span>> {
    language.and_then(|language| highlight_with_syntax(code, language, theme)).unwrap_or_else(|| plain(code, theme.palette.text))
}

fn highlight_with_syntax(code: &str, language: &str, theme: &Theme) -> Option<Vec<Vec<Span>>> {
    let syntax = language::find(language, &SYNTAXES)?;
    let syntax_theme = THEMES.get(theme.syntax)?;
    let mut highlighter = HighlightLines::new(syntax, syntax_theme);
    LinesWithEndings::from(code)
        .map(|line| highlighter.highlight_line(line, &SYNTAXES).ok().map(|regions| spans(&regions, theme.palette.surface)))
        .collect()
}

fn plain(code: &str, color: Rgb) -> Vec<Vec<Span>> {
    code.lines().map(|line| non_empty_span(line, Style::fg(color)).into_iter().collect()).collect()
}

fn spans(regions: &[(SyntaxStyle, &str)], surface: Rgb) -> Vec<Span> {
    regions.iter().filter_map(|(style, text)| non_empty_span(text, style_from(*style, surface))).collect()
}

fn non_empty_span(text: &str, style: Style) -> Option<Span> {
    let text = text.trim_end_matches(['\n', '\r']).replace('\t', TAB);
    (!text.is_empty()).then(|| Span::new(text, style))
}

fn style_from(syntax: SyntaxStyle, surface: Rgb) -> Style {
    Style {
        fg: Some(blend(syntax.foreground, surface)),
        bold: syntax.font_style.contains(FontStyle::BOLD),
        italic: syntax.font_style.contains(FontStyle::ITALIC),
        underline: syntax.font_style.contains(FontStyle::UNDERLINE),
        ..Style::default()
    }
}

/// Some themes fade a colour with alpha; the terminal has no alpha, so it is mixed into the panel background.
fn blend(color: Color, background: Rgb) -> Rgb {
    let mix = |front: u8, back: u8| ((u16::from(front) * u16::from(color.a) + u16::from(back) * (255 - u16::from(color.a))) / 255) as u8;
    Rgb(mix(color.r, background.0), mix(color.g, background.1), mix(color.b, background.2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::MRK_DARK;

    fn foreground_of(lines: &[Vec<Span>], text: &str) -> Option<Rgb> {
        lines.iter().flatten().find(|span| span.text.contains(text)).and_then(|span| span.style.fg)
    }

    #[test]
    fn a_keyword_and_a_string_get_different_colours() {
        let lines = highlight("fn main() {\n    let greeting = \"hello\";\n}\n", Some("rust"), &MRK_DARK);

        let keyword = foreground_of(&lines, "fn");
        let string = foreground_of(&lines, "hello");
        assert!(keyword.is_some() && string.is_some());
        assert_ne!(keyword, string);
    }

    #[test]
    fn unknown_language_is_plain_text_colour() {
        let lines = highlight("let a = \"b\";\n", Some("klingon"), &MRK_DARK);

        assert_eq!(lines, vec![vec![Span::new("let a = \"b\";", Style::fg(MRK_DARK.palette.text))]]);
    }

    #[test]
    fn tabs_expand_and_line_endings_go() {
        let lines = highlight("\tx\r\n\n", None, &MRK_DARK);

        assert_eq!(lines, vec![vec![Span::new("    x", Style::fg(MRK_DARK.palette.text))], vec![]]);
    }

    #[test]
    fn highlighted_lines_keep_tabs_expanded() {
        let lines = highlight("fn a() {\n\treturn;\n}\n", Some("rs"), &MRK_DARK);
        let second: String = lines[1].iter().map(|span| span.text.as_str()).collect();

        assert_eq!(second, "    return;");
    }

    #[test]
    fn font_styles_map_onto_the_style() {
        let syntax = SyntaxStyle {
            foreground: Color { r: 1, g: 2, b: 3, a: 255 },
            background: Color::BLACK,
            font_style: FontStyle::BOLD | FontStyle::ITALIC | FontStyle::UNDERLINE,
        };

        let style = style_from(syntax, Rgb(0, 0, 0));

        assert_eq!(style, Style::fg(Rgb(1, 2, 3)).bold().italic().underline());
    }

    #[test]
    fn a_faded_colour_mixes_into_the_background() {
        let half_white = Color { r: 255, g: 255, b: 255, a: 128 };

        assert_eq!(blend(half_white, Rgb(0, 0, 0)), Rgb(128, 128, 128));
    }
}
