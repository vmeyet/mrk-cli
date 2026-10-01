use crate::document::Rgb;

const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
const CUBE_START: u8 = 16;
const GRAY_START: u8 = 232;
const GRAY_STEPS: u8 = 24;

fn nearest_cube_step(channel: u8) -> u8 {
    (0..6u8).min_by_key(|&step| CUBE_LEVELS[usize::from(step)].abs_diff(channel)).unwrap_or(0)
}

fn gray_level(step: u8) -> u8 {
    8 + 10 * step
}

pub fn distance(a: Rgb, b: Rgb) -> u32 {
    let square = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    square(a.0, b.0) + square(a.1, b.1) + square(a.2, b.2)
}

fn cube_candidate(color: Rgb) -> (u8, Rgb) {
    let (red, green, blue) = (nearest_cube_step(color.0), nearest_cube_step(color.1), nearest_cube_step(color.2));
    let index = CUBE_START + 36 * red + 6 * green + blue;
    let level = |step: u8| CUBE_LEVELS[usize::from(step)];
    (index, Rgb(level(red), level(green), level(blue)))
}

fn gray_candidate(color: Rgb) -> (u8, Rgb) {
    let average = ((u16::from(color.0) + u16::from(color.1) + u16::from(color.2)) / 3) as u8;
    let step = (0..GRAY_STEPS).min_by_key(|&step| gray_level(step).abs_diff(average)).unwrap_or(0);
    let level = gray_level(step);
    (GRAY_START + step, Rgb(level, level, level))
}

/// The xterm-256 index closest to `color`, from the 6×6×6 cube and the gray ramp (the 16 system colours vary per terminal).
pub fn ansi256(color: Rgb) -> u8 {
    let (cube, cube_color) = cube_candidate(color);
    let (gray, gray_color) = gray_candidate(color);
    if distance(color, gray_color) < distance(color, cube_color) { gray } else { cube }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_corners_map_exactly() {
        assert_eq!(ansi256(Rgb(0, 0, 0)), 16);
        assert_eq!(ansi256(Rgb(255, 255, 255)), 231);
        assert_eq!(ansi256(Rgb(255, 0, 0)), 196);
        assert_eq!(ansi256(Rgb(0, 255, 0)), 46);
        assert_eq!(ansi256(Rgb(0, 0, 255)), 21);
    }

    #[test]
    fn a_cube_colour_maps_to_itself() {
        assert_eq!(ansi256(Rgb(95, 135, 175)), 16 + 36 + 2 * 6 + 3);
    }

    #[test]
    fn grays_prefer_the_gray_ramp() {
        assert_eq!(ansi256(Rgb(128, 128, 128)), 244);
        assert_eq!(ansi256(Rgb(0x1f, 0x23, 0x2e)), 235);
    }

    #[test]
    fn a_palette_colour_lands_near_its_hue() {
        assert_eq!(ansi256(Rgb(0x8a, 0xb4, 0xf8)), 111);
    }
}
