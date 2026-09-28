use crate::theme::Theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub dim: bool,
}

impl Style {
    pub fn fg(color: Rgb) -> Self {
        Self { fg: Some(color), ..Self::default() }
    }

    pub fn on(self, color: Rgb) -> Self {
        Self { bg: Some(color), ..self }
    }

    pub fn bold(self) -> Self {
        Self { bold: true, ..self }
    }

    pub fn italic(self) -> Self {
        Self { italic: true, ..self }
    }

    pub fn underline(self) -> Self {
        Self { underline: true, ..self }
    }

    pub fn strike(self) -> Self {
        Self { strike: true, ..self }
    }

    pub fn dim(self) -> Self {
        Self { dim: true, ..self }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self { text: text.into(), style, link: None }
    }

    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Style::default())
    }

    pub fn linked(self, target: impl Into<String>) -> Self {
        Self { link: Some(target.into()), ..self }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    pub fn new(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    pub fn blank() -> Self {
        Self::default()
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(|span| crate::text::display_width(&span.text)).sum()
    }

    pub fn plain(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// A PNG and the terminal cell box it is drawn into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub png: Vec<u8>,
    pub cols: u16,
    pub rows: u16,
    pub alt: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Lines(Vec<Line>),
    Picture(Picture),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

/// Size of one terminal cell in pixels; present only when pictures can be drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSize {
    pub width_px: u16,
    pub height_px: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub width: usize,
    pub theme: Theme,
    pub cell: Option<CellSize>,
}

/// The document as bare text, one `[picture: alt cols×rows]` line per picture: what snapshot tests compare.
pub fn plain(document: &Document) -> String {
    document.blocks.iter().map(plain_block).collect()
}

fn plain_block(block: &Block) -> String {
    match block {
        Block::Lines(lines) => lines.iter().map(|line| format!("{}\n", line.plain().trim_end())).collect(),
        Block::Picture(picture) => format!("[picture: {} {}×{}]\n", picture.alt, picture.cols, picture.rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_joins_spans_and_trims_trailing_padding() {
        let line = Line::new(vec![Span::plain("a "), Span::plain("b   ")]);
        let document = Document { blocks: vec![Block::Lines(vec![line, Line::blank()])] };

        assert_eq!(plain(&document), "a b\n\n");
    }

    #[test]
    fn line_width_counts_cells_not_bytes() {
        let line = Line::new(vec![Span::plain("é漢")]);

        assert_eq!(line.width(), 3);
    }
}
