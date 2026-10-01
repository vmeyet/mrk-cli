use crate::document::CellSize;
use crate::raster::MAX_SIDE_PX;

/// A cell is about 1.2 font sizes tall, so this ratio turns cell height into font size.
const CELL_HEIGHT_PER_FONT_SIZE: f32 = 1.2;

/// How a diagram of a given size is scaled into whole terminal cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub scale: f32,
    pub cols: u16,
    pub rows: u16,
    pub width_px: u32,
    pub height_px: u32,
}

/// Scales the diagram so its labels match the terminal font, shrunk to fit `max_cols` and the pixel cap; `None` when nothing fits.
pub fn fit(width: f32, height: f32, font_size_px: f32, cell: CellSize, max_cols: usize) -> Option<Frame> {
    let is_drawable = width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0 && font_size_px > 0.0;
    let has_cells = cell.width_px > 0 && cell.height_px > 0 && max_cols > 0;
    if !is_drawable || !has_cells {
        return None;
    }

    let max_cols = max_cols.min(whole_cells(MAX_SIDE_PX, cell.width_px));
    let max_rows = whole_cells(MAX_SIDE_PX, cell.height_px);
    let scale = [
        f32::from(cell.height_px) / CELL_HEIGHT_PER_FONT_SIZE / font_size_px,
        (max_cols as f32 * f32::from(cell.width_px)) / width,
        (max_rows as f32 * f32::from(cell.height_px)) / height,
    ]
    .into_iter()
    .fold(f32::INFINITY, f32::min);
    let cols = cells_covering(width * scale, cell.width_px).min(max_cols);
    let rows = cells_covering(height * scale, cell.height_px).min(max_rows);

    frame(scale, cols, rows, cell)
}

fn whole_cells(pixels: u32, cell_px: u16) -> usize {
    (pixels / u32::from(cell_px)) as usize
}

fn cells_covering(pixels: f32, cell_px: u16) -> usize {
    (pixels / f32::from(cell_px)).ceil() as usize
}

fn frame(scale: f32, cols: usize, rows: usize, cell: CellSize) -> Option<Frame> {
    let cols = u16::try_from(cols).ok().filter(|cols| *cols > 0)?;
    let rows = u16::try_from(rows).ok().filter(|rows| *rows > 0)?;
    let width_px = u32::from(cols) * u32::from(cell.width_px);
    let height_px = u32::from(rows) * u32::from(cell.height_px);
    Some(Frame { scale, cols, rows, width_px, height_px })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const CELL: CellSize = CellSize { width_px: 10, height_px: 24 };

    #[test]
    fn labels_match_the_terminal_font() {
        let frame = fit(200.0, 100.0, 14.0, CELL, 80).unwrap();

        assert!((frame.scale * 14.0 - 20.0).abs() < 0.01);
        assert_eq!((frame.cols, frame.rows), (29, 6));
        assert_eq!((frame.width_px, frame.height_px), (290, 144));
    }

    #[test]
    fn wide_diagrams_shrink_to_the_width() {
        let frame = fit(2000.0, 100.0, 14.0, CELL, 80).unwrap();

        assert_eq!(frame.cols, 80);
        assert!((frame.scale - 0.4).abs() < 0.001);
        assert_eq!(frame.rows, 2);
    }

    #[test]
    fn tall_diagrams_stay_under_the_pixel_cap() {
        let frame = fit(100.0, 100_000.0, 14.0, CELL, 80).unwrap();

        assert!(frame.height_px <= MAX_SIDE_PX);
        assert_eq!(frame.rows, 170);
    }

    #[test]
    fn huge_cells_still_respect_the_pixel_cap() {
        let cell = CellSize { width_px: 100, height_px: 200 };
        let frame = fit(10_000.0, 100.0, 14.0, cell, 80).unwrap();

        assert!(frame.width_px <= MAX_SIDE_PX);
    }

    #[test]
    fn degenerate_sizes_do_not_fit() {
        assert_eq!(fit(0.0, 100.0, 14.0, CELL, 80), None);
        assert_eq!(fit(f32::NAN, 100.0, 14.0, CELL, 80), None);
        assert_eq!(fit(100.0, 100.0, 14.0, CellSize { width_px: 0, height_px: 24 }, 80), None);
        assert_eq!(fit(100.0, 100.0, 14.0, CELL, 0), None);
    }
}
