//! Znak wodny dla podglądów grupy D: napis wbudowaną czcionką bitmapową 5×7,
//! powtarzany w przesuniętych rzędach na całym obrazie. Bez plików fontów
//! i dodatkowych zależności; napis jest częścią pikseli, więc nie da się go
//! zdjąć jak metadanych.

use image::{Rgb, RgbImage};

/// Tekst znaku wodnego (tylko znaki z [`glyph`]).
pub const TEXT: &str = "KS MATCHDAY PODGLAD";

const GLYPH_WIDTH: u32 = 5;
const GLYPH_HEIGHT: u32 = 7;
/// Krycie napisu (0–255) i jego cienia, który poprawia czytelność na jasnym tle.
const TEXT_ALPHA: u32 = 110;
const SHADOW_ALPHA: u32 = 70;

/// Wiersze glifu 5×7 (najstarszy z 5 bitów = lewa kolumna).
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        ' ' => [0; 7],
        _ => return None,
    })
}

fn blend(pixel: &mut Rgb<u8>, color: [u8; 3], alpha: u32) {
    for (channel, target) in pixel.0.iter_mut().zip(color) {
        let mixed = (u32::from(*channel) * (255 - alpha) + u32::from(target) * alpha) / 255;
        *channel = u8::try_from(mixed).unwrap_or(u8::MAX);
    }
}

/// Rysuje jeden napis w (x, y) w skali `scale` (piksel glifu = kwadrat scale×scale).
fn draw_text(image: &mut RgbImage, x: i64, y: i64, scale: u32, color: [u8; 3], alpha: u32) {
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));
    let step = i64::from((GLYPH_WIDTH + 1) * scale);
    for (index, c) in TEXT.chars().enumerate() {
        let Some(rows) = glyph(c) else { continue };
        let left = x + i64::try_from(index).unwrap_or(0) * step;
        for (row, bits) in (0..).zip(rows) {
            for column in 0..GLYPH_WIDTH {
                if bits & (1 << (GLYPH_WIDTH - 1 - column)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = left + i64::from(column * scale + dx);
                        let py = y + i64::from(row * scale + dy);
                        if (0..width).contains(&px) && (0..height).contains(&py) {
                            // Granice sprawdzone wyżej, więc konwersje się udają.
                            if let (Ok(px), Ok(py)) = (u32::try_from(px), u32::try_from(py)) {
                                blend(image.get_pixel_mut(px, py), color, alpha);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Szerokość napisu w pikselach przy danej skali.
fn text_width(scale: u32) -> u32 {
    u32::try_from(TEXT.chars().count()).unwrap_or(0) * (GLYPH_WIDTH + 1) * scale
}

/// Nakłada powtarzany napis na cały obraz.
pub fn apply(image: &mut RgbImage) {
    let scale = (image.width().max(image.height()) / 300).max(2);
    let line_width = i64::from(text_width(scale));
    let gap_x = line_width / 3;
    let gap_y = i64::from(GLYPH_HEIGHT * scale * 5);
    let (width, height) = (i64::from(image.width()), i64::from(image.height()));

    let mut row = 0;
    let mut y = gap_y / 2;
    while y < height {
        // Co drugi rząd przesunięty, żeby napis nie układał się w kolumny.
        let mut x = if row % 2 == 0 {
            0
        } else {
            -(line_width + gap_x) / 2
        };
        while x < width {
            let shadow = i64::from(scale.max(2) / 2);
            draw_text(image, x + shadow, y + shadow, scale, [0, 0, 0], SHADOW_ALPHA);
            draw_text(image, x, y, scale, [255, 255, 255], TEXT_ALPHA);
            x += line_width + gap_x;
        }
        y += gap_y;
        row += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_character_of_the_text_has_a_glyph() {
        assert!(TEXT.chars().all(|c| glyph(c).is_some()), "brak glifu w {TEXT}");
    }

    #[test]
    fn watermark_covers_the_whole_image() {
        let mut image = RgbImage::from_pixel(1200, 800, Rgb([40, 40, 40]));
        apply(&mut image);
        let changed = |x0: u32, y0: u32, x1: u32, y1: u32| {
            (y0..y1).any(|y| (x0..x1).any(|x| image.get_pixel(x, y) != &Rgb([40, 40, 40])))
        };
        // Każda ćwiartka obrazu zawiera fragment znaku wodnego.
        assert!(changed(0, 0, 600, 400));
        assert!(changed(600, 0, 1200, 400));
        assert!(changed(0, 400, 600, 800));
        assert!(changed(600, 400, 1200, 800));
    }

    #[test]
    fn works_on_tiny_images() {
        let mut image = RgbImage::from_pixel(3, 2, Rgb([0, 0, 0]));
        apply(&mut image);
    }
}
