use super::page::Placed;
use crate::terminal::kitty::{self, Crop, Placement};

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const IHDR_WIDTH: std::ops::Range<usize> = 16..20;
const IHDR_HEIGHT: std::ops::Range<usize> = 20..24;

/// The rows of a picture that are on screen: `skip` rows hidden above the top, then `rows` visible from `screen_row`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    pub screen_row: usize,
    pub skip: u16,
    pub rows: u16,
}

/// Which rows of a picture starting at `first_row` and `rows` tall show between `top` and `top + height`.
pub fn visible(first_row: usize, rows: u16, top: usize, height: usize) -> Option<Visible> {
    let last_row = first_row + usize::from(rows);
    let start = first_row.max(top);
    let end = last_row.min(top + height);
    (start < end).then(|| Visible { screen_row: start - top, skip: (start - first_row) as u16, rows: (end - start) as u16 })
}

fn big_endian(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

/// Width and height in pixels, read from the PNG header.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let is_png = png.starts_with(PNG_SIGNATURE);
    let size = (big_endian(png.get(IHDR_WIDTH)?)?, big_endian(png.get(IHDR_HEIGHT)?)?);
    is_png.then_some(size)
}

fn pixel_row(row: u16, rows: u16, height_px: u32) -> u32 {
    (u64::from(row) * u64::from(height_px) / u64::from(rows.max(1))) as u32
}

/// The band of pixels behind the visible rows, when the PNG's `height_px` spreads evenly over the picture's `rows`.
pub fn crop(visible: Visible, rows: u16, (width_px, height_px): (u32, u32)) -> Crop {
    let top = pixel_row(visible.skip, rows, height_px);
    let bottom = pixel_row(visible.skip + visible.rows, rows, height_px);
    Crop { x: 0, y: top, width: width_px, height: bottom - top }
}

/// Where and how to place the picture so only its on-screen rows are drawn, `None` when it is off screen.
pub fn placement(placed: &Placed, id: u32, top: usize, height: usize) -> Option<(usize, Placement)> {
    let picture = &placed.picture;
    let rows = picture.rows.max(1);
    let shown = visible(placed.row, rows, top, height)?;
    let size = png_size(&picture.png)?;
    let crop = crop(shown, rows, size);
    Some((shown.screen_row, Placement { id, crop, cols: picture.cols, rows: shown.rows, z: kitty::z_index(picture) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Line, Picture};

    fn png(width: u32, height: u32) -> Vec<u8> {
        [PNG_SIGNATURE, b"\0\0\0\rIHDR", &width.to_be_bytes(), &height.to_be_bytes(), b"rest"].concat()
    }

    #[test]
    fn a_fully_visible_picture_shows_every_row() {
        assert_eq!(visible(5, 4, 2, 10), Some(Visible { screen_row: 3, skip: 0, rows: 4 }));
    }

    #[test]
    fn a_picture_scrolled_past_the_top_is_clipped_above() {
        assert_eq!(visible(5, 4, 7, 10), Some(Visible { screen_row: 0, skip: 2, rows: 2 }));
    }

    #[test]
    fn a_picture_below_the_bottom_is_clipped_below() {
        assert_eq!(visible(5, 4, 0, 7), Some(Visible { screen_row: 5, skip: 0, rows: 2 }));
    }

    #[test]
    fn a_picture_taller_than_the_screen_is_clipped_on_both_sides() {
        assert_eq!(visible(0, 20, 5, 10), Some(Visible { screen_row: 0, skip: 5, rows: 10 }));
    }

    #[test]
    fn a_picture_off_screen_is_hidden() {
        assert_eq!(visible(5, 4, 9, 10), None);
        assert_eq!(visible(5, 4, 0, 5), None);
    }

    #[test]
    fn the_png_size_comes_from_its_header() {
        assert_eq!(png_size(&png(400, 120)), Some((400, 120)));
        assert_eq!(png_size(b"GIF89a but long enough to have a header"), None);
        assert_eq!(png_size(PNG_SIGNATURE), None);
    }

    #[test]
    fn the_crop_is_the_pixel_band_behind_the_visible_rows() {
        let size = (400, 120);

        assert_eq!(crop(Visible { screen_row: 0, skip: 0, rows: 6 }, 6, size), Crop { x: 0, y: 0, width: 400, height: 120 });
        assert_eq!(crop(Visible { screen_row: 0, skip: 2, rows: 4 }, 6, size), Crop { x: 0, y: 40, width: 400, height: 80 });
        assert_eq!(crop(Visible { screen_row: 3, skip: 0, rows: 1 }, 6, size), Crop { x: 0, y: 0, width: 400, height: 20 });
    }

    #[test]
    fn uneven_rows_tile_the_png_without_gaps() {
        let size = (10, 100);
        let bands: Vec<Crop> = (0..3).map(|skip| crop(Visible { screen_row: 0, skip, rows: 1 }, 3, size)).collect();

        assert_eq!(bands.iter().map(|band| (band.y, band.height)).collect::<Vec<_>>(), [(0, 33), (33, 33), (66, 34)]);
    }

    #[test]
    fn a_placement_uses_the_visible_rows_and_the_picture_columns() {
        let placed = Placed {
            row: 10,
            picture: Picture { png: png(400, 120), cols: 40, rows: 6, alt: "flow".to_owned(), indent: Line::blank(), concealed_text: None },
        };

        assert_eq!(
            placement(&placed, 3, 12, 20),
            Some((0, Placement { id: 3, crop: Crop { x: 0, y: 40, width: 400, height: 80 }, cols: 40, rows: 4, z: 0 }))
        );
        assert_eq!(placement(&placed, 3, 30, 20), None);
    }
}
