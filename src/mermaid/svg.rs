use std::panic::{self, AssertUnwindSafe};

use mermaid_rs_renderer::{LayoutConfig, RenderOptions, Theme};

use super::DiagramError;
use crate::document::Rgb;
use crate::theme::Palette;

pub const FONT_FAMILY: &str =
    "Inter, \"Helvetica Neue\", Helvetica, \"DejaVu Sans\", \"Noto Sans\", \"Liberation Sans\", Arial, sans-serif";
pub const FONT_SIZE_PX: f32 = 14.0;
const TRANSPARENT: &str = "none";

/// Mermaid source drawn as an SVG in the palette's colours on a transparent background.
pub fn render(source: &str, palette: &Palette) -> Result<String, DiagramError> {
    let options = RenderOptions { theme: themed(palette), layout: layout() };
    let rendered = panic::catch_unwind(AssertUnwindSafe(|| mermaid_rs_renderer::render_with_options(source, options)));
    match rendered {
        Ok(Ok(svg)) => Ok(with_accents(&svg, palette)),
        Ok(Err(error)) => Err(DiagramError::Invalid(first_line(&error.to_string()))),
        Err(_) => Err(DiagramError::Crashed),
    }
}

// Decision shapes are the only polygons drawn straight on the canvas; arrowheads are polygons in a transformed group or marker paths.
fn with_accents(svg: &str, palette: &Palette) -> String {
    let accent = hex(palette.accent);
    let style =
        format!("<style>svg > polygon {{ stroke: {accent}; }} g > polygon, marker path {{ fill: {accent}; stroke: {accent}; }}</style>");
    match svg.split_once('>') {
        Some((opening, rest)) => format!("{opening}>{style}{rest}"),
        None => svg.to_owned(),
    }
}

fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or("invalid diagram").to_owned()
}

// Fast metrics keep the engine off its own font cache under `~/.cache` for ASCII labels.
fn layout() -> LayoutConfig {
    LayoutConfig { fast_text_metrics: true, ..LayoutConfig::default() }
}

fn hex(color: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", color.0, color.1, color.2)
}

fn relative_luminance(color: Rgb) -> f32 {
    let channel = |value: u8| f32::from(value) / 255.0;
    0.2126 * channel(color.0) + 0.7152 * channel(color.1) + 0.0722 * channel(color.2)
}

fn ink_on(fill: Rgb, palette: &Palette) -> String {
    let fill_luminance = relative_luminance(fill);
    let text_contrast = (relative_luminance(palette.text) - fill_luminance).abs();
    let surface_contrast = (relative_luminance(palette.surface) - fill_luminance).abs();
    let ink = if text_contrast >= surface_contrast { palette.text } else { palette.surface };
    hex(ink)
}

fn series(palette: &Palette) -> [Rgb; 12] {
    [
        palette.accent,
        palette.h1,
        palette.h3,
        palette.warning,
        palette.caution,
        palette.success,
        palette.link,
        palette.code,
        palette.important,
        palette.tip,
        palette.note,
        palette.muted,
    ]
}

fn git_colors(palette: &Palette) -> [String; 8] {
    let colors = series(palette);
    std::array::from_fn(|index| hex(colors[index]))
}

fn git_label_colors(palette: &Palette) -> [String; 8] {
    let colors = series(palette);
    std::array::from_fn(|index| ink_on(colors[index], palette))
}

fn themed(palette: &Palette) -> Theme {
    let text = hex(palette.text);
    let muted = hex(palette.muted);
    let subtle = hex(palette.subtle);
    let surface = hex(palette.surface);
    let accent = hex(palette.accent);
    Theme {
        font_family: FONT_FAMILY.to_owned(),
        font_size: FONT_SIZE_PX,
        primary_color: surface.clone(),
        primary_text_color: text.clone(),
        primary_border_color: muted.clone(),
        line_color: muted.clone(),
        secondary_color: surface.clone(),
        tertiary_color: surface.clone(),
        edge_label_background: surface.clone(),
        cluster_background: TRANSPARENT.to_owned(),
        cluster_border: subtle.clone(),
        background: TRANSPARENT.to_owned(),
        sequence_actor_fill: surface.clone(),
        sequence_actor_border: muted.clone(),
        sequence_actor_line: subtle.clone(),
        sequence_note_fill: surface.clone(),
        sequence_note_border: accent.clone(),
        sequence_activation_fill: subtle.clone(),
        sequence_activation_border: muted.clone(),
        text_color: text.clone(),
        git_colors: git_colors(palette),
        git_inv_colors: git_label_colors(palette),
        git_branch_label_colors: git_label_colors(palette),
        git_commit_label_color: text.clone(),
        git_commit_label_background: surface.clone(),
        git_tag_label_color: text.clone(),
        git_tag_label_background: surface.clone(),
        git_tag_label_border: accent,
        pie_colors: series(palette).map(hex),
        pie_title_text_size: FONT_SIZE_PX * 1.4,
        pie_title_text_color: text.clone(),
        pie_section_text_size: FONT_SIZE_PX,
        pie_section_text_color: hex(palette.surface),
        pie_legend_text_size: FONT_SIZE_PX,
        pie_legend_text_color: text,
        pie_stroke_color: surface,
        pie_stroke_width: 1.5,
        pie_outer_stroke_width: 1.5,
        pie_outer_stroke_color: subtle,
        pie_opacity: 0.9,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::theme::MRK_DARK;

    #[test]
    fn flowchart_uses_the_palette_on_a_transparent_background() {
        let svg = render("graph TD\n  A[Start] --> B{Ready?}", &MRK_DARK.palette).unwrap();

        assert!(svg.contains(&hex(MRK_DARK.palette.text)));
        assert!(svg.contains(&hex(MRK_DARK.palette.muted)));
        assert!(svg.contains("fill=\"none\""));
        assert!(!svg.contains("#FFFFFF"));
    }

    #[test]
    fn invalid_source_is_an_error() {
        assert!(matches!(render("hello", &MRK_DARK.palette), Err(DiagramError::Invalid(_))));
    }

    #[test]
    fn labels_on_bright_fills_take_the_darker_ink() {
        assert_eq!(ink_on(MRK_DARK.palette.warning, &MRK_DARK.palette), hex(MRK_DARK.palette.surface));
        assert_eq!(ink_on(MRK_DARK.palette.subtle, &MRK_DARK.palette), hex(MRK_DARK.palette.text));
    }
}
