#![allow(clippy::unwrap_used)]

use mrk::document::{Block, CellSize, Settings, plain};
use mrk::markdown;
use mrk::theme::test_settings;

const SHOWCASE: &str = include_str!("fixtures/showcase.md");
const CELL: CellSize = CellSize { width_px: 10, height_px: 22 };

fn widest_line(settings: &Settings) -> usize {
    let document = markdown::render(SHOWCASE, settings);
    document
        .blocks
        .iter()
        .filter_map(|block| if let Block::Lines(lines) = block { Some(lines) } else { None })
        .flatten()
        .map(mrk::document::Line::width)
        .max()
        .unwrap()
}

#[test]
fn the_showcase_renders_as_text_without_pictures() {
    insta::assert_snapshot!(plain(&markdown::render(SHOWCASE, &test_settings())));
}

#[test]
fn the_showcase_draws_both_diagrams_as_pictures_when_the_terminal_can() {
    let settings = Settings { cell: Some(CELL), ..test_settings() };

    let pictures: Vec<_> = markdown::render(SHOWCASE, &settings)
        .blocks
        .into_iter()
        .filter_map(|block| if let Block::Picture(p) = block { Some(p) } else { None })
        .collect();

    assert_eq!(pictures.iter().map(|p| p.alt.as_str()).collect::<Vec<_>>(), ["flowchart", "sequence diagram"]);
    assert!(pictures.iter().all(|p| p.png.starts_with(b"\x89PNG") && usize::from(p.cols) <= settings.width));
}

#[test]
fn no_showcase_line_is_wider_than_the_width() {
    for width in [20, 33, 60, 80, 100] {
        let settings = Settings { width, ..test_settings() };

        assert!(widest_line(&settings) <= width, "width {width}");
    }
}
