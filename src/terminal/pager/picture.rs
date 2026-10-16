use super::page::Placed;
use crate::terminal::kitty::{self, Crop, Placement};

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

/// Where and how to place the picture so only its on-screen rows are drawn, `None` when it is off screen.
pub fn placement(placed: &Placed, id: u32, top: usize, height: usize) -> Option<(usize, Placement)> {
    let picture = &placed.picture;
    let rows = picture.rows.max(1);
    let shown = visible(placed.row, rows, top, height)?;
    let crop = Crop::behind(shown.skip..shown.skip + shown.rows, rows, &picture.png)?;
    Some((shown.screen_row, Placement { id, crop, cols: picture.cols, rows: shown.rows, z: kitty::z_index(picture) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Line, Picture};

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
    fn a_placement_uses_the_visible_rows_and_the_picture_columns() {
        let placed = Placed {
            row: 10,
            picture: Picture {
                png: crate::terminal::sixel::test_png(400, 120, &vec![[0; 4]; 400 * 120]),
                cols: 40,
                rows: 6,
                alt: "flow".to_owned(),
                indent: Line::blank(),
                concealed_text: None,
            },
        };

        assert_eq!(
            placement(&placed, 3, 12, 20),
            Some((0, Placement { id: 3, crop: Crop { x: 0, y: 40, width: 400, height: 80 }, cols: 40, rows: 4, z: 0 }))
        );
        assert_eq!(placement(&placed, 3, 30, 20), None);
    }
}
