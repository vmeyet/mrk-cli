use super::{FONT_FAMILY, MAX_SIDE_PX, RasterError, Svg};
use crate::document::{CellSize, Line, Picture, Rgb};
use crate::text;

/// Font size per pixel of picture height, leaving room for ascenders above and descenders below the baseline.
const FONT_SIZE_PER_HEIGHT: f32 = 0.75;
/// Capital letters stand about 0.7 font sizes tall, so a baseline this far below the middle centres them.
const BASELINE_BELOW_MIDDLE: f32 = 0.35;

/// `text` on one line `rows` cells tall in `color`, bold where the font has a bold face, shrunk to fit `max_cols`.
pub fn line(text: &str, color: Rgb, cell: CellSize, rows: u16, max_cols: usize) -> Result<Picture, RasterError> {
    let height_px = u32::from(rows) * u32::from(cell.height_px);
    let max_cols = max_cols.min((MAX_SIDE_PX / u32::from(cell.width_px.max(1))) as usize);
    let max_width_px = max_cols as f32 * f32::from(cell.width_px);
    let drawing = fitted(text, color, height_px, max_width_px)?;
    let cols = (drawing.ink_right() / f32::from(cell.width_px)).ceil().clamp(1.0, max_cols.max(1) as f32) as u16;
    let png = drawing.draw(1.0, u32::from(cols) * u32::from(cell.width_px), height_px)?;
    Ok(Picture { png, cols, rows, alt: text.to_owned(), indent: Line::blank(), concealed_text: None })
}

/// The text at the size that fills the height, or smaller when that would be wider than `max_width_px`.
fn fitted(text: &str, color: Rgb, height_px: u32, max_width_px: f32) -> Result<Svg, RasterError> {
    let font_size = height_px as f32 * FONT_SIZE_PER_HEIGHT;
    let natural = Svg::parse(&svg(text, color, font_size, max_width_px, height_px), text)?;
    let width = natural.ink_right();
    if width <= max_width_px {
        return Ok(natural);
    }
    Svg::parse(&svg(text, color, font_size * max_width_px / width, max_width_px, height_px), text)
}

fn svg(text: &str, color: Rgb, font_size: f32, width_px: f32, height_px: u32) -> String {
    let baseline = height_px as f32 / 2.0 + font_size * BASELINE_BELOW_MIDDLE;
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width_px}" height="{height_px}"><text x="0" y="{baseline}" xml:space="preserve" font-family='{FONT_FAMILY}' font-weight="bold" font-size="{font_size}" fill="{}">{}</text></svg>"#,
        color.hex(),
        escaped(text)
    )
}

/// The text as XML character data: markup escaped, and the characters `terminal::sanitize` removes from the concealed
/// copy dropped, controls that XML forbids among them.
fn escaped(text: &str) -> String {
    text.chars()
        .filter(|&character| !text::is_forbidden(character))
        .map(|character| match character {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::raster::inspect;

    const CELL: CellSize = CellSize { width_px: 10, height_px: 22 };
    const BLUE: Rgb = Rgb(0x82, 0xaa, 0xff);

    #[test]
    fn the_line_fills_whole_cells_rows_tall() {
        let picture = line("Getting started", BLUE, CELL, 2, 40).unwrap();
        let drawn = inspect(&picture.png).unwrap();

        assert_eq!(picture.rows, 2);
        assert_eq!((drawn.width, drawn.height), (u32::from(picture.cols) * 10, 44));
        assert!(drawn.has_ink);
        assert!(picture.cols > 15 && picture.cols <= 40, "cols {}", picture.cols);
        assert_eq!(picture.alt, "Getting started");
    }

    #[test]
    fn a_line_too_wide_shrinks_to_the_columns() {
        let picture = line("A title far too long for ten columns", BLUE, CELL, 2, 10).unwrap();

        assert_eq!(picture.cols, 10);
        assert!(inspect(&picture.png).unwrap().has_ink);
    }

    #[test]
    fn markup_in_the_text_is_drawn_as_text() {
        let picture = line("<image href=\"/etc/passwd\"/> & co", BLUE, CELL, 2, 80).unwrap();

        assert!(picture.cols > 20, "cols {}", picture.cols);
        assert_eq!(escaped("a<b>&\u{1b}c"), "a&lt;b&gt;&amp;c");
    }
}
