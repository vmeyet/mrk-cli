use std::sync::Arc;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::fontdb::Database;
use resvg::usvg::{ImageHrefResolver, Options, Tree};

use super::DiagramError;
use super::frame::Frame;

/// An SVG parsed with every external resource refused: fonts come only from `fonts`.
pub fn parse(svg: &str, fonts: Arc<Database>) -> Result<Tree, DiagramError> {
    Tree::from_str(svg, &isolated_options(fonts)).map_err(|error| DiagramError::UnreadableSvg(error.to_string()))
}

/// The tree drawn at the frame's scale onto a transparent canvas of the frame's pixel size, as PNG bytes.
pub fn draw(tree: &Tree, frame: &Frame) -> Result<Vec<u8>, DiagramError> {
    let mut pixmap = Pixmap::new(frame.width_px, frame.height_px).ok_or(DiagramError::Empty)?;
    resvg::render(tree, Transform::from_scale(frame.scale, frame.scale), &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|error| DiagramError::PngEncoding(error.to_string()))
}

fn isolated_options(fonts: Arc<Database>) -> Options<'static> {
    let refuse_every_image = ImageHrefResolver { resolve_data: Box::new(|_, _, _| None), resolve_string: Box::new(|_, _| None) };
    Options { resources_dir: None, image_href_resolver: refuse_every_image, fontdb: fonts, ..Options::default() }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use base64::Engine;

    use super::*;

    const FRAME: Frame = Frame { scale: 1.0, cols: 2, rows: 1, width_px: 20, height_px: 20 };
    const RED_SVG: &str =
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#f00"/></svg>"##;

    fn parsed(svg: &str) -> Tree {
        parse(svg, Arc::new(Database::new())).unwrap()
    }

    fn drawn_pixels(svg: &str) -> Pixmap {
        Pixmap::decode_png(&draw(&parsed(svg), &FRAME).unwrap()).unwrap()
    }

    fn image_svg(href: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="20" height="20"><image href="{href}" xlink:href="{href}" width="20" height="20"/></svg>"#
        )
    }

    fn is_blank(pixmap: &Pixmap) -> bool {
        pixmap.pixels().iter().all(|pixel| pixel.alpha() == 0)
    }

    fn is_refused(href: &str) -> bool {
        let svg = image_svg(href);
        !parsed(&svg).root().has_children() && is_blank(&drawn_pixels(&svg))
    }

    #[test]
    fn shapes_are_drawn_at_the_frame_size() {
        let pixmap = drawn_pixels(RED_SVG);

        assert_eq!((pixmap.width(), pixmap.height()), (20, 20));
        assert!(!is_blank(&pixmap));
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
