use std::ops::Range;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const IHDR_WIDTH: Range<usize> = 16..20;
const IHDR_HEIGHT: Range<usize> = 20..24;

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
    u32::try_from(u64::from(row) * u64::from(height_px) / u64::from(rows.max(1))).unwrap_or(height_px)
}

/// A part of a PNG, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Crop {
    /// The band of pixels behind `shown`, rows of a picture `rows` tall, when the PNG's height spreads evenly over
    /// them; `None` when the PNG has no readable header.
    pub fn behind(shown: Range<u16>, rows: u16, png: &[u8]) -> Option<Self> {
        let (width_px, height_px) = png_size(png)?;
        let top = pixel_row(shown.start, rows, height_px);
        let bottom = pixel_row(shown.end, rows, height_px);
        Some(Self { x: 0, y: top, width: width_px, height: bottom - top })
    }

    pub fn pixel_rows(self) -> Range<u32> {
        self.y..self.y + self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        [PNG_SIGNATURE, b"\0\0\0\rIHDR", &width.to_be_bytes(), &height.to_be_bytes(), b"rest"].concat()
    }

    #[test]
    fn the_png_size_comes_from_its_header() {
        assert_eq!(png_size(&png(400, 120)), Some((400, 120)));
        assert_eq!(png_size(b"GIF89a but long enough to have a header"), None);
        assert_eq!(png_size(PNG_SIGNATURE), None);
    }

    #[test]
    fn the_crop_is_the_pixel_band_behind_the_visible_rows() {
        let png = png(400, 120);

        assert_eq!(Crop::behind(0..6, 6, &png), Some(Crop { x: 0, y: 0, width: 400, height: 120 }));
        assert_eq!(Crop::behind(2..6, 6, &png), Some(Crop { x: 0, y: 40, width: 400, height: 80 }));
        assert_eq!(Crop::behind(0..1, 6, &png), Some(Crop { x: 0, y: 0, width: 400, height: 20 }));
    }

    #[test]
    fn uneven_rows_tile_the_png_without_gaps() {
        let png = png(10, 100);
        let bands: Vec<Crop> = (0..3).filter_map(|skip| Crop::behind(skip..skip + 1, 3, &png)).collect();

        assert_eq!(bands.iter().map(|band| (band.y, band.height)).collect::<Vec<_>>(), [(0, 33), (33, 33), (66, 34)]);
    }
}
