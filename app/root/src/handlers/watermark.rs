use std::{env, fs};
use image::{Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};
use tracing::{info, warn};

const DEFAULT_WATERMARK_FONT_PATH: &str = "public/fonts/Roboto-Bold.ttf";

pub fn load_watermark_font() -> Option<Font<'static>> {
    let font_path = env::var("WATERMARK_FONT_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_WATERMARK_FONT_PATH.to_string());

    match fs::read(&font_path) {
        Ok(font_data) => match Font::try_from_vec(font_data) {
            Some(font) => {
                info!("Loaded watermark font from {}", font_path);
                Some(font)
            }
            None => {
                warn!(
                    "Failed to parse watermark font from {}. Continuing without watermark.",
                    font_path
                );
                None
            }
        },
        Err(error) => {
            warn!(
                "Failed to read watermark font from {}: {}. Continuing without watermark.",
                font_path,
                error
            );
            None
        }
    }
}

pub fn apply_watermark(image: &mut RgbaImage, font: &Font) {
    let width = image.width() as f32;
    let height = image.height() as f32;

    let font_size = 48.0;
    let scale = Scale::uniform(font_size);

    let text = "gallery.example";

    let glyphs: Vec<_> = font.layout(text, scale, rusttype::point(0.0, 0.0)).collect();
    let text_width = glyphs
        .iter()
        .last()
        .map(|g| g.position().x + g.unpositioned().h_metrics().advance_width)
        .unwrap_or(0.0);

    let x_bottom_left = 5;
    let y_bottom_left = (height - font_size - 5.0) as i32;
    
    let x_top_right = (width - text_width - 5.0) as i32;
    let y_top_right = 5;

    let white = Rgba([255u8, 255u8, 255u8, 255u8]);
    let black = Rgba([0u8, 0u8, 0u8, 255u8]);

    let positions = vec![
        (x_bottom_left, y_bottom_left),
        (x_top_right, y_top_right),
    ];

    for (x, y) in positions {
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 { continue; }
                draw_text_mut(image, white, x + dx, y + dy, scale, font, text);
            }
        }
        draw_text_mut(image, black, x, y, scale, font, text);
    }
}
