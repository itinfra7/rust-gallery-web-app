use std::{fs, path::Path, io::Cursor};
use image::io::Reader as ImageReader;
use crate::handlers::watermark::{apply_watermark, load_watermark_font};
use crate::utils::generate_complex_filename;

pub fn process_static<F>(path: &Path, update_status: F) -> anyhow::Result<Option<String>> 
where F: Fn(&str) {
    update_status("Decoding Image...");
    let data = fs::read(path)?;
    let img_res = ImageReader::new(Cursor::new(&data)).with_guessed_format()?.decode();

    if let Ok(img) = img_res {
        let new_name = generate_complex_filename();
        let thumb_name = new_name.replace(".webp", "_thumb.webp");
        
        let mut rgba = img.to_rgba8();
        let thumb_src = rgba.clone();
        let watermark_font = load_watermark_font();

        if let Some(font) = watermark_font.as_ref() {
            apply_watermark(&mut rgba, font);
        }

        update_status("Encoding WebP...");
        let output_path = Path::new("uploads").join(&new_name);
        let thumb_path = Path::new("uploads/thumbs").join(&thumb_name);

        let (w, h) = rgba.dimensions();
        let raw_data = rgba.into_raw();
        let encoder = webp::Encoder::from_rgba(&raw_data, w, h);
        let encoded_image = encoder.encode(80.0);

        let thumb = image::DynamicImage::ImageRgba8(thumb_src).thumbnail(500, 500);
        let (tw, th) = (thumb.width(), thumb.height());
        let thumb_data = thumb.to_rgba8().into_raw();
        let thumb_enc = webp::Encoder::from_rgba(&thumb_data, tw, th);
        let encoded_thumb = thumb_enc.encode(70.0);

        let write_result: anyhow::Result<()> = (|| {
            fs::write(&output_path, &*encoded_image)?;
            fs::write(&thumb_path, &*encoded_thumb)?;
            Ok(())
        })();

        if let Err(error) = write_result {
            if output_path.exists() {
                fs::remove_file(&output_path).ok();
            }
            if thumb_path.exists() {
                fs::remove_file(&thumb_path).ok();
            }
            return Err(error);
        }

        Ok(Some(new_name))
    } else {
        Ok(None)
    }
}
