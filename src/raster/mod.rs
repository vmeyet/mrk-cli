//! SVG to PNG: the one module that sees resvg and the fonts it draws text with.
mod fonts;
mod line;

use std::sync::Arc;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::fontdb::Database;
use resvg::usvg::{ImageHrefResolver, Options, Tree};

pub use self::fonts::FONT_FAMILY;
pub use self::line::line;

/// Pixels per side a picture may reach (`specs/02-security.md` rule 4).
pub const MAX_SIDE_PX: u32 = 4096;

/// Why an SVG did not become a PNG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterError {
    UnreadableSvg(String),
    /// The canvas has no pixels or is over `MAX_SIDE_PX` on a side.
    NoCanvas,
    PngEncoding(String),
}

/// An SVG parsed with every external resource refused, its fonts picked for the text it shows.
pub struct Svg(Tree);

impl Svg {
    /// `text` is every character the SVG may draw, so fonts covering it get loaded.
    pub fn parse(svg: &str, text: &str) -> Result<Self, RasterError> {
        parse_with(svg, fonts::for_text(text)).map(Self)
    }

    /// Width and height in SVG units.
    pub fn size(&self) -> (f32, f32) {
        let size = self.0.size();
        (size.width(), size.height())
    }

    /// The right edge of what the SVG draws, in SVG units.
    fn ink_right(&self) -> f32 {
        self.0.root().abs_bounding_box().right()
    }

    /// Drawn at `scale` onto a transparent canvas of that many pixels, as PNG bytes.
    pub fn draw(&self, scale: f32, width_px: u32, height_px: u32) -> Result<Vec<u8>, RasterError> {
        if width_px > MAX_SIDE_PX || height_px > MAX_SIDE_PX {
            return Err(RasterError::NoCanvas);
        }
        let mut pixmap = Pixmap::new(width_px, height_px).ok_or(RasterError::NoCanvas)?;
        resvg::render(&self.0, Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        pixmap.encode_png().map_err(|error| RasterError::PngEncoding(error.to_string()))
    }
}

/// An SVG parsed with every external resource refused: fonts come only from `fonts`.
fn parse_with(svg: &str, fonts: Arc<Database>) -> Result<Tree, RasterError> {
    Tree::from_str(svg, &isolated_options(fonts)).map_err(|error| RasterError::UnreadableSvg(error.to_string()))
}

fn isolated_options(fonts: Arc<Database>) -> Options<'static> {
    let refuse_every_image = ImageHrefResolver { resolve_data: Box::new(|_, _, _| None), resolve_string: Box::new(|_, _| None) };
    Options { resources_dir: None, image_href_resolver: refuse_every_image, fontdb: fonts, ..Options::default() }
}

/// What a PNG mrk drew holds, for tests that check a picture without decoding it themselves.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inspected {
    pub width: u32,
    pub height: u32,
    pub has_ink: bool,
}

#[cfg(test)]
pub fn inspect(png: &[u8]) -> Option<Inspected> {
    let pixmap = Pixmap::decode_png(png).ok()?;
    let has_ink = pixmap.pixels().iter().any(|pixel| pixel.alpha() > 0);
    Some(Inspected { width: pixmap.width(), height: pixmap.height(), has_ink })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use base64::Engine;

    use super::*;

    const RED_SVG: &str =
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#f00"/></svg>"##;

    fn parsed(svg: &str) -> Svg {
        Svg(parse_with(svg, Arc::new(Database::new())).unwrap())
    }

    fn drawn(svg: &str) -> Inspected {
        inspect(&parsed(svg).draw(1.0, 20, 20).unwrap()).unwrap()
    }

    fn image_svg(href: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="20" height="20"><image href="{href}" xlink:href="{href}" width="20" height="20"/></svg>"#
        )
    }

    fn is_refused(href: &str) -> bool {
        let svg = image_svg(href);
        !parsed(&svg).0.root().has_children() && !drawn(&svg).has_ink
    }

    #[test]
    fn shapes_are_drawn_at_the_canvas_size() {
        assert_eq!(drawn(RED_SVG), Inspected { width: 20, height: 20, has_ink: true });
    }

    #[test]
    fn a_canvas_over_the_pixel_cap_is_refused() {
        assert_eq!(parsed(RED_SVG).draw(1.0, MAX_SIDE_PX + 1, 20), Err(RasterError::NoCanvas));
        assert_eq!(parsed(RED_SVG).draw(1.0, 0, 20), Err(RasterError::NoCanvas));
    }

    #[test]
    fn local_files_are_never_loaded() {
        let path = std::env::temp_dir().join("mrk-raster-test-red.svg");
        std::fs::write(&path, RED_SVG).unwrap();

        assert!(is_refused(path.to_str().unwrap()));
        assert!(is_refused(&format!("file://{}", path.display())));
        assert!(is_refused("file:///etc/passwd"));
        assert!(is_refused("/etc/passwd"));
    }

    #[test]
    fn local_files_load_without_the_guard() {
        let path = std::env::temp_dir().join("mrk-raster-test-control.svg");
        std::fs::write(&path, RED_SVG).unwrap();

        let tree = Tree::from_str(&image_svg(path.to_str().unwrap()), &Options::default()).unwrap();

        assert!(tree.root().has_children());
    }

    #[test]
    fn data_urls_are_never_loaded() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(RED_SVG);

        assert!(is_refused(&format!("data:image/svg+xml;base64,{encoded}")));
    }

    #[test]
    fn remote_urls_are_never_loaded() {
        assert!(is_refused("https://example.com/image.svg"));
    }
}
