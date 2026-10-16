use std::sync::LazyLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, FontStyle, Style as SyntaxStyle};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

use super::language;
use crate::document::{Rgb, Span, Style};
use crate::theme::{SyntaxTheme, Theme};

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);
static THEMES: LazyLock<EmbeddedLazyThemeSet> = LazyLock::new(two_face::theme::extra);

/// One entry per source line, highlighted as it is taken so a long block is never held twice: no line ending, no
/// background. A language without a syntax gives plain text in the palette's `text`, and so does a line that fails.
pub fn highlight<'a>(code: &'a str, language: Option<&str>, theme: &Theme) -> Box<dyn Iterator<Item = Vec<Span>> + 'a> {
    let (text, surface) = (theme.palette.text, theme.palette.surface);
    match language.and_then(|language| language::find(language, &SYNTAXES)) {
        Some(syntax) => {
            let mut highlighter = HighlightLines::new(syntax, THEMES.get(embedded(theme.syntax)));
            Box::new(LinesWithEndings::from(code).map(move |line| match highlighter.highlight_line(line, &SYNTAXES) {
                Ok(regions) => spans(&regions, surface),
                Err(_) => plain(line, text),
            }))
        }
        None => Box::new(code.lines().map(move |line| plain(line, text))),
    }
}

fn embedded(theme: SyntaxTheme) -> EmbeddedThemeName {
    match theme {
        SyntaxTheme::CatppuccinLatte => EmbeddedThemeName::CatppuccinLatte,
        SyntaxTheme::CatppuccinMacchiato => EmbeddedThemeName::CatppuccinMacchiato,
        SyntaxTheme::CatppuccinMocha => EmbeddedThemeName::CatppuccinMocha,
        SyntaxTheme::Dracula => EmbeddedThemeName::Dracula,
        SyntaxTheme::GitHub => EmbeddedThemeName::Github,
        SyntaxTheme::GruvboxDark => EmbeddedThemeName::GruvboxDark,
        SyntaxTheme::GruvboxLight => EmbeddedThemeName::GruvboxLight,
        SyntaxTheme::Nord => EmbeddedThemeName::Nord,
        SyntaxTheme::OneHalfDark => EmbeddedThemeName::OneHalfDark,
        SyntaxTheme::OneHalfLight => EmbeddedThemeName::OneHalfLight,
        SyntaxTheme::TwoDark => EmbeddedThemeName::TwoDark,
    }
}

fn plain(line: &str, color: Rgb) -> Vec<Span> {
    non_empty_span(line, Style::fg(color)).into_iter().collect()
}

fn spans(regions: &[(SyntaxStyle, &str)], surface: Rgb) -> Vec<Span> {
    regions.iter().filter_map(|(style, text)| non_empty_span(text, style_from(*style, surface))).collect()
}

fn non_empty_span(text: &str, style: Style) -> Option<Span> {
    let text = text.trim_end_matches(['\n', '\r']);
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
    background.mix(Rgb(color.r, color.g, color.b), f32::from(color.a) / f32::from(u8::MAX))
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
        let lines = highlight("fn main() {\n    let greeting = \"hello\";\n}\n", Some("rust"), &MRK_DARK).collect::<Vec<_>>();

        let keyword = foreground_of(&lines, "fn");
        let string = foreground_of(&lines, "hello");
        assert!(keyword.is_some() && string.is_some());
        assert_ne!(keyword, string);
    }

    #[test]
    fn every_theme_highlights_with_its_syntax_theme() {
        for theme in crate::theme::names().filter_map(crate::theme::find) {
            let lines: Vec<Vec<Span>> = highlight("fn main() {}\n", Some("rust"), &theme).collect();

            assert_ne!(foreground_of(&lines, "fn"), Some(theme.palette.text), "{}", theme.name);
        }
    }

    #[test]
    fn unknown_language_is_plain_text_colour() {
        let lines: Vec<Vec<Span>> = highlight("let a = \"b\";\n", Some("klingon"), &MRK_DARK).collect();

        assert_eq!(lines, vec![vec![Span::new("let a = \"b\";", Style::fg(MRK_DARK.palette.text))]]);
    }

    #[test]
    fn line_endings_go() {
        let lines: Vec<Vec<Span>> = highlight("x\r\n\n", None, &MRK_DARK).collect();

        assert_eq!(lines, vec![vec![Span::new("x", Style::fg(MRK_DARK.palette.text))], vec![]]);
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
