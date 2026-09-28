use crate::document::{Block, Document, Line, Picture};

/// One screen row of the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Line(Line),
    /// A row covered by the picture at this index of `Page::pictures`.
    Picture(usize),
}

/// A picture and the first row it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub row: usize,
    pub picture: Picture,
}

/// The document as the pager scrolls it: one entry per screen row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    pub rows: Vec<Row>,
    pub pictures: Vec<Placed>,
}

fn picture_rows(picture: &Picture, index: usize) -> impl Iterator<Item = Row> + use<> {
    std::iter::repeat_n(Row::Picture(index), usize::from(picture.rows.max(1)))
}

pub fn flatten(document: Document) -> Page {
    let mut page = Page::default();
    for block in document.blocks {
        match block {
            Block::Lines(lines) => page.rows.extend(lines.into_iter().map(Row::Line)),
            Block::Picture(picture) => {
                page.rows.extend(picture_rows(&picture, page.pictures.len()));
                let row = page.rows.len() - usize::from(picture.rows.max(1));
                page.pictures.push(Placed { row, picture });
            }
        }
    }
    page
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Span;

    fn line(text: &str) -> Line {
        Line::new(vec![Span::plain(text)])
    }

    fn picture(rows: u16) -> Picture {
        Picture { png: vec![], cols: 10, rows, alt: "flow".to_owned() }
    }

    #[test]
    fn lines_are_one_row_each_and_a_picture_spans_its_rows() {
        let document = Document {
            blocks: vec![
                Block::Lines(vec![line("a"), line("b")]),
                Block::Picture(picture(3)),
                Block::Lines(vec![line("c")]),
                Block::Picture(picture(2)),
            ],
        };
        let page = flatten(document);

        assert_eq!(
            page.rows,
            [
                Row::Line(line("a")),
                Row::Line(line("b")),
                Row::Picture(0),
                Row::Picture(0),
                Row::Picture(0),
                Row::Line(line("c")),
                Row::Picture(1),
                Row::Picture(1)
            ]
        );
        assert_eq!(page.pictures.iter().map(|placed| placed.row).collect::<Vec<_>>(), [2, 6]);
    }

    #[test]
    fn a_picture_without_rows_still_takes_one() {
        let page = flatten(Document { blocks: vec![Block::Picture(picture(0))] });

        assert_eq!(page.rows, [Row::Picture(0)]);
    }
}
