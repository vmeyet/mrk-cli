use std::borrow::Cow;

use super::color::ansi256;
use super::{Capabilities, ColorDepth, sanitize};
use crate::document::{Line, Rgb, Span, Style};

const RESET: &str = "\x1b[0m";
const HYPERLINK_CLOSE: &str = "\x1b]8;;\x1b\\";

#[derive(Default)]
struct Pen {
    style: Style,
    link: Option<String>,
}

fn toggle(from: bool, to: bool, on: &'static str, off: &'static str) -> Option<Cow<'static, str>> {
    (from != to).then_some(Cow::Borrowed(if to { on } else { off }))
}

fn color_code(color: Option<Rgb>, depth: ColorDepth, layer: u8) -> Cow<'static, str> {
    match (color, depth) {
        (None, _) | (_, ColorDepth::None) => Cow::Borrowed(if layer == 3 { "39" } else { "49" }),
        (Some(Rgb(red, green, blue)), ColorDepth::TrueColor) => Cow::Owned(format!("{layer}8;2;{red};{green};{blue}")),
        (Some(color), ColorDepth::Ansi256) => Cow::Owned(format!("{layer}8;5;{}", ansi256(color))),
    }
}

fn intensity_codes(from: Style, to: Style) -> [Option<Cow<'static, str>>; 3] {
    let loses_intensity = (from.bold && !to.bold) || (from.dim && !to.dim);
    let reset = loses_intensity.then_some(Cow::Borrowed("22"));
    let bold = (to.bold && (loses_intensity || !from.bold)).then_some(Cow::Borrowed("1"));
    let dim = (to.dim && (loses_intensity || !from.dim)).then_some(Cow::Borrowed("2"));
    [reset, bold, dim]
}

fn color_change(from: Option<Rgb>, to: Option<Rgb>, depth: ColorDepth, layer: u8) -> Option<Cow<'static, str>> {
    (from != to).then(|| color_code(to, depth, layer))
}

fn sgr(from: Style, to: Style, depth: ColorDepth) -> String {
    let changes = [
        toggle(from.italic, to.italic, "3", "23"),
        toggle(from.underline, to.underline, "4", "24"),
        toggle(from.strike, to.strike, "9", "29"),
        color_change(from.fg, to.fg, depth, 3),
        color_change(from.bg, to.bg, depth, 4),
    ];
    let codes: Vec<Cow<'static, str>> = intensity_codes(from, to).into_iter().chain(changes).flatten().collect();
    if codes.is_empty() { String::new() } else { format!("\x1b[{}m", codes.join(";")) }
}

fn hyperlink(from: Option<&str>, to: Option<&str>) -> String {
    match to {
        _ if from == to => String::new(),
        Some(target) => format!("\x1b]8;;{target}\x1b\\"),
        None => HYPERLINK_CLOSE.to_owned(),
    }
}

fn visible_style(style: Style, depth: ColorDepth) -> Style {
    if depth == ColorDepth::None { Style::default() } else { style }
}

fn visible_link(span: &Span, capabilities: &Capabilities) -> Option<String> {
    span.link.as_deref().filter(|_| capabilities.hyperlinks).and_then(sanitize::link)
}

fn close(pen: &Pen) -> String {
    let link = if pen.link.is_some() { HYPERLINK_CLOSE } else { "" };
    let style = if pen.style == Style::default() { "" } else { RESET };
    format!("{link}{style}")
}

/// One line behind the margin, attributes emitted only when they change, everything closed before the newline
/// so a background or a hyperlink never runs past the line.
pub fn line(line: &Line, capabilities: &Capabilities, margin: usize) -> String {
    if line.spans.is_empty() {
        return "\n".to_owned();
    }

    let mut out = " ".repeat(margin);
    let mut pen = Pen::default();
    let mut pen_source_link: Option<&str> = None;
    for span in &line.spans {
        let text = sanitize::text(&span.text);
        if text.is_empty() {
            continue;
        }
        let link = if span.link.as_deref() == pen_source_link { pen.link.clone() } else { visible_link(span, capabilities) };
        pen_source_link = span.link.as_deref();
        let next = Pen { style: visible_style(span.style, capabilities.color), link };
        out.push_str(&hyperlink(pen.link.as_deref(), next.link.as_deref()));
        out.push_str(&sgr(pen.style, next.style, capabilities.color));
        out.push_str(&text);
        pen = next;
    }
    out.push_str(&close(&pen));
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Span;

    fn line(line: &Line, capabilities: &Capabilities) -> String {
        super::line(line, capabilities, crate::terminal::LEFT_MARGIN)
    }

    const RED: Rgb = Rgb(255, 0, 0);

    fn capabilities(color: ColorDepth, hyperlinks: bool) -> Capabilities {
        Capabilities { color, hyperlinks, graphics: None, background: None, columns: 80, is_terminal: true }
    }

    fn truecolor() -> Capabilities {
        capabilities(ColorDepth::TrueColor, true)
    }

    #[test]
    fn a_blank_line_has_no_margin() {
        assert_eq!(line(&Line::blank(), &truecolor()), "\n");
    }

    #[test]
    fn plain_spans_get_the_margin_and_no_escape() {
        assert_eq!(line(&Line::new(vec![Span::plain("a"), Span::plain("b")]), &truecolor()), "  ab\n");
    }

    #[test]
    fn unchanged_attributes_are_not_emitted_again() {
        let style = Style::fg(RED).bold();
        let out = line(&Line::new(vec![Span::new("a", style), Span::new("b", style)]), &truecolor());

        assert_eq!(out, "  \x1b[1;38;2;255;0;0mab\x1b[0m\n");
    }

    #[test]
    fn only_the_changed_attribute_is_emitted() {
        let out = line(&Line::new(vec![Span::new("a", Style::fg(RED)), Span::new("b", Style::fg(RED).italic())]), &truecolor());

        assert_eq!(out, "  \x1b[38;2;255;0;0ma\x1b[3mb\x1b[0m\n");
    }

    #[test]
    fn dropping_bold_keeps_dim() {
        let out =
            line(&Line::new(vec![Span::new("a", Style::default().bold().dim()), Span::new("b", Style::default().dim())]), &truecolor());

        assert_eq!(out, "  \x1b[1;2ma\x1b[22;2mb\x1b[0m\n");
    }

    #[test]
    fn attributes_turn_off_individually() {
        let styled = Style::fg(RED).on(RED).italic().underline().strike();
        let out = line(&Line::new(vec![Span::new("a", styled), Span::plain("b")]), &truecolor());

        assert_eq!(out, "  \x1b[3;4;9;38;2;255;0;0;48;2;255;0;0ma\x1b[23;24;29;39;49mb\n");
    }

    #[test]
    fn a_background_is_reset_before_the_newline() {
        let out = line(&Line::new(vec![Span::new("code ", Style::default().on(RED))]), &truecolor());

        assert!(out.ends_with("\x1b[0m\n"));
    }

    #[test]
    fn ansi256_downgrades_colours() {
        let out = line(&Line::new(vec![Span::new("a", Style::fg(RED).on(Rgb(128, 128, 128)))]), &capabilities(ColorDepth::Ansi256, false));

        assert_eq!(out, "  \x1b[38;5;196;48;5;244ma\x1b[0m\n");
    }

    #[test]
    fn no_colour_means_plain_text_and_no_hyperlink() {
        let span = Span::new("a", Style::fg(RED).bold().underline()).linked("https://x.y");

        assert_eq!(line(&Line::new(vec![span]), &capabilities(ColorDepth::None, false)), "  a\n");
    }

    #[test]
    fn a_link_opens_once_across_spans_and_closes_at_line_end() {
        let spans = vec![Span::plain("see ").linked("https://x.y"), Span::new("docs", Style::default().bold()).linked("https://x.y")];

        assert_eq!(line(&Line::new(spans), &truecolor()), "  \x1b]8;;https://x.y\x1b\\see \x1b[1mdocs\x1b]8;;\x1b\\\x1b[0m\n");
    }

    #[test]
    fn a_link_closes_before_unlinked_text() {
        let spans = vec![Span::plain("a").linked("https://x.y"), Span::plain("b")];

        assert_eq!(line(&Line::new(spans), &truecolor()), "  \x1b]8;;https://x.y\x1b\\a\x1b]8;;\x1b\\b\n");
    }

    #[test]
    fn links_are_off_without_the_capability() {
        let span = Span::plain("a").linked("https://x.y");

        assert_eq!(line(&Line::new(vec![span]), &capabilities(ColorDepth::TrueColor, false)), "  a\n");
    }

    #[test]
    fn a_rejected_link_keeps_its_text() {
        let span = Span::plain("click").linked("javascript:alert(1)");

        assert_eq!(line(&Line::new(vec![span]), &truecolor()), "  click\n");
    }

    #[test]
    fn hostile_span_text_reaches_the_terminal_defused() {
        let span = Span::plain("\x1b]52;c;AAAA\x07\x1b[2J\u{9b}31m\u{202e}x");

        assert_eq!(line(&Line::new(vec![span]), &truecolor()), "  ]52;c;AAAA[2J31mx\n");
    }
}
