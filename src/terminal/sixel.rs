use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::ops::Range;

use super::color::distance;
use crate::document::{Picture, Rgb};

/// Sixel has 256 colour registers on the terminals that matter.
const MAX_COLORS: usize = 256;
/// Below half opacity a pixel is left out, so the terminal background shows through: Sixel has no partial alpha.
const MIN_ALPHA: u8 = 128;
/// One sixel is a column of six pixels.
const BAND_PX: u32 = 6;
const SIXEL_OFFSET: u8 = 0x3f;
/// Colours are counted on 5 bits per channel, so the near-identical shades of anti-aliasing share one bucket.
const BUCKET_BITS: u8 = 5;
const BUCKETS: usize = 1 << (3 * BUCKET_BITS);
/// Shorter runs cost fewer bytes written out than as `!count`.
const MIN_REPEAT: usize = 4;
/// `P2=1`: pixels no colour sets stay transparent.
const BEGIN: &str = "\x1bP0;1;0q";
const END: &str = "\x1b\\";

/// A picture's pixels reduced to at most 256 colours, ready to be sent as Sixel, whole or a band of rows at a time.
#[derive(Clone, Debug)]
pub struct Image {
    width: u32,
    height: u32,
    /// The palette as Sixel colour definitions, sent ahead of the bands every time.
    colors: String,
    /// One palette index per pixel, row after row; `None` where the pixel is transparent.
    pixels: Vec<Option<u8>>,
}

impl Image {
    /// `None` unless the PNG is 8-bit RGBA, the only kind mrk draws.
    pub fn from_png(png: &[u8]) -> Option<Self> {
        let (width, height, rgba) = decode(png)?;
        Some(quantize(width, height, &rgba))
    }

    /// The pixel `rows` as one Sixel sequence, cut to whole bands of six so the image never reaches into the text row
    /// below it, which would scroll the screen at the bottom.
    pub fn encode(&self, rows: Range<u32>) -> String {
        let top = rows.start.min(self.height);
        let height = rows.end.min(self.height).saturating_sub(top) / BAND_PX * BAND_PX;
        let bands: String = (top..top + height).step_by(BAND_PX as usize).map(|band_top| self.band(band_top)).collect();
        format!("{BEGIN}\"1;1;{};{height}{}{bands}{END}", self.width, self.colors)
    }

    fn row(&self, y: u32) -> &[Option<u8>] {
        let width = self.width as usize;
        &self.pixels[y as usize * width..][..width]
    }

    /// Six rows from `top`: each colour's sixels in turn, back to the band's start (`$`) between colours, then down (`-`).
    fn band(&self, top: u32) -> String {
        let mut masks: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
        for (bit, y) in (top..top + BAND_PX).enumerate() {
            for (x, index) in self.row(y).iter().enumerate().filter_map(|(x, index)| Some((x, (*index)?))) {
                masks.entry(index).or_insert_with(|| vec![0; self.width as usize])[x] |= 1 << bit;
            }
        }
        let colors: Vec<String> = masks.iter().map(|(index, mask)| format!("#{index}{}", run_length(mask))).collect();
        format!("{}-", colors.join("$"))
    }
}

/// Draws the picture with Sixel from the cursor, then puts the cursor back where it was: where it ends up after an
/// image differs between terminals. `None` when the PNG cannot be read.
pub fn draw(picture: &Picture) -> Option<String> {
    let image = Image::from_png(&picture.png)?;
    Some(format!("\x1b7{}\x1b8", image.encode(0..image.height)))
}

fn decode(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut reader = png::Decoder::new(Cursor::new(png)).read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let is_rgba = info.color_type == png::ColorType::Rgba && info.bit_depth == png::BitDepth::Eight;
    buffer.truncate(info.buffer_size());
    is_rgba.then_some((info.width, info.height, buffer))
}

fn is_opaque(pixel: &[u8]) -> bool {
    pixel[3] >= MIN_ALPHA
}

fn bucket(pixel: &[u8]) -> usize {
    let shift = 8 - BUCKET_BITS;
    pixel[..3].iter().fold(0, |bucket, channel| (bucket << BUCKET_BITS) | usize::from(channel >> shift))
}

#[derive(Clone, Copy, Default)]
struct Tally {
    count: u32,
    sums: [u64; 3],
}

impl Tally {
    fn mean(self) -> Rgb {
        let channel = |sum: u64| (sum / u64::from(self.count.max(1))) as u8;
        Rgb(channel(self.sums[0]), channel(self.sums[1]), channel(self.sums[2]))
    }
}

fn tallies(rgba: &[u8]) -> Vec<Tally> {
    let mut tallies = vec![Tally::default(); BUCKETS];
    for pixel in rgba.chunks_exact(4).filter(|pixel| is_opaque(pixel)) {
        let tally = &mut tallies[bucket(pixel)];
        tally.count += 1;
        tally.sums.iter_mut().zip(&pixel[..3]).for_each(|(sum, channel)| *sum += u64::from(*channel));
    }
    tallies
}

fn nearest(color: Rgb, palette: &[Rgb]) -> u8 {
    (0..palette.len()).min_by_key(|&index| distance(color, palette[index])).unwrap_or_default() as u8
}

/// The most common colour buckets become the palette, each at the mean of its pixels; every other bucket takes the
/// nearest of them. Diagrams use a handful of flat colours, which this keeps exact.
fn quantize(width: u32, height: u32, rgba: &[u8]) -> Image {
    let tallies = tallies(rgba);
    let mut used: Vec<usize> = (0..BUCKETS).filter(|&bucket| tallies[bucket].count > 0).collect();
    used.sort_by_key(|&bucket| (Reverse(tallies[bucket].count), bucket));
    let (common, rare) = used.split_at(used.len().min(MAX_COLORS));
    let palette: Vec<Rgb> = common.iter().map(|&bucket| tallies[bucket].mean()).collect();

    let mut lookup = vec![0; BUCKETS];
    for (index, &bucket) in common.iter().enumerate() {
        lookup[bucket] = index as u8;
    }
    for &bucket in rare {
        lookup[bucket] = nearest(tallies[bucket].mean(), &palette);
    }
    let pixels = rgba.chunks_exact(4).map(|pixel| is_opaque(pixel).then(|| lookup[bucket(pixel)])).collect();
    Image { width, height, colors: colors(&palette), pixels }
}

fn colors(palette: &[Rgb]) -> String {
    let percent = |channel: u8| u32::from(channel) * 100 / 255;
    let define =
        |(index, Rgb(red, green, blue)): (usize, &Rgb)| format!("#{index};2;{};{};{}", percent(*red), percent(*green), percent(*blue));
    palette.iter().enumerate().map(define).collect()
}

fn run_length(mask: &[u8]) -> String {
    let end = mask.iter().rposition(|&bits| bits != 0).map_or(0, |last| last + 1);
    let mut runs = Vec::new();
    let mut rest = &mask[..end];
    while let Some(&bits) = rest.first() {
        let run = rest.iter().take_while(|&&each| each == bits).count();
        let sixel = char::from(SIXEL_OFFSET + bits);
        runs.push(if run >= MIN_REPEAT { format!("!{run}{sixel}") } else { sixel.to_string().repeat(run) });
        rest = &rest[run..];
    }
    runs.concat()
}

/// An 8-bit RGBA PNG of these pixels, row after row, for tests.
#[cfg(test)]
pub fn test_png(width: u32, height: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let written = encoder.write_header().and_then(|mut writer| writer.write_image_data(&pixels.concat()));
    assert!(written.is_ok(), "{written:?}");
    bytes
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn image(width: u32, height: u32, pixels: &[[u8; 4]]) -> Image {
        Image::from_png(&test_png(width, height, pixels)).unwrap()
    }

    fn readable(sixel: &str) -> String {
        sixel.replace('\x1b', "␛")
    }

    #[test]
    fn a_two_colour_image_is_one_band_per_six_rows() {
        let left_red_right_blue: Vec<[u8; 4]> = (0..12).flat_map(|_| [RED, RED, RED, RED, BLUE]).collect();

        insta::assert_snapshot!(readable(&image(5, 12, &left_red_right_blue).encode(0..12)));
    }

    #[test]
    fn transparent_pixels_get_no_colour() {
        let image = image(2, 6, &[[RED, CLEAR]; 6].concat());

        assert_eq!(image.encode(0..6), "\x1bP0;1;0q\"1;1;2;6#0;2;100;0;0#0~-\x1b\\");
    }

    #[test]
    fn half_transparent_pixels_count_as_opaque() {
        let image = image(1, 6, &[[255, 0, 0, 128], [255, 0, 0, 127], RED, RED, RED, RED]);

        assert_eq!(image.encode(0..6), "\x1bP0;1;0q\"1;1;1;6#0;2;100;0;0#0|-\x1b\\");
    }

    #[test]
    fn rows_are_cut_to_whole_bands_inside_the_image() {
        let image = image(1, 14, &[RED; 14]);

        assert!(image.encode(0..14).contains("\"1;1;1;12#"));
        assert!(image.encode(3..10).contains("\"1;1;1;6#"));
        assert!(image.encode(10..40).contains("\"1;1;1;0#"));
    }

    #[test]
    fn long_runs_are_repeated_and_trailing_blanks_dropped() {
        assert_eq!(run_length(&[1, 1, 1, 1, 1, 2, 2, 0, 0]), "!5@AA");
        assert_eq!(run_length(&[0, 0, 0]), "");
    }

    #[test]
    fn many_colours_are_reduced_to_256() {
        let gradient: Vec<[u8; 4]> = (0..=255).flat_map(|red| (0..4).map(move |blue| [red, 0, blue * 80, 255])).collect();

        let image = image(32, 32, &gradient);

        assert!(image.colors.matches(";2;").count() <= MAX_COLORS);
        assert!(image.pixels.iter().all(Option::is_some));
    }

    #[test]
    fn a_rare_colour_takes_the_nearest_common_one() {
        let palette = [Rgb(255, 0, 0), Rgb(0, 0, 255)];

        assert_eq!(nearest(Rgb(200, 10, 30), &palette), 0);
        assert_eq!(nearest(Rgb(20, 10, 220), &palette), 1);
    }

    #[test]
    fn only_rgba_pngs_are_read() {
        assert!(Image::from_png(b"not a png").is_none());
        let mut gray = Vec::new();
        let mut encoder = png::Encoder::new(&mut gray, 1, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.write_header().unwrap().write_image_data(&[0]).unwrap();

        assert!(Image::from_png(&gray).is_none());
    }

    #[test]
    fn a_drawn_picture_puts_the_cursor_back() {
        let picture =
            Picture { png: test_png(1, 6, &[RED; 6]), cols: 1, rows: 1, alt: "flow".to_owned(), indent: crate::document::Line::blank() };

        let drawn = draw(&picture).unwrap();

        assert!(drawn.starts_with("\x1b7\x1bP") && drawn.ends_with("\x1b\\\x1b8"), "{drawn:?}");
    }
}
