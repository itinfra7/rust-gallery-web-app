use std::{fs, path::Path, io::Cursor};
use image::AnimationDecoder;
use webp_animation::{Encoder, EncoderOptions, EncodingConfig, EncodingType, LossyEncodingConfig};
use crate::handlers::watermark::{apply_watermark, load_watermark_font};
use crate::utils::generate_complex_filename;

pub fn process_animated<F>(path: &Path, update_status: F) -> anyhow::Result<Option<String>> 
where F: Fn(&str) {
    update_status("Decoding GIF...");
    let data = fs::read(path)?;
    let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(&data))?;
    let frames = decoder.into_frames().collect_frames()?;

    if frames.is_empty() { return Ok(None); }

    let new_name = generate_complex_filename().replace(".webp", "_anim.webp");
    let thumb_name = new_name.replace(".webp", "_thumb.webp");
    let watermark_font = load_watermark_font();

    let (w, h) = frames[0].buffer().dimensions();
    let mut encoder = Encoder::new_with_options((w, h), EncoderOptions {
        encoding_config: Some(EncodingConfig {
            encoding_type: EncodingType::Lossy(LossyEncodingConfig::default()),
            quality: 75.0,
            method: 4,
            ..Default::default()
        }),
        ..Default::default()
    })?;

    let mut timestamp_ms = 0;
    update_status("Encoding Frames...");
    
    for frame in &frames {
        let (num, den) = frame.delay().numer_denom_ms();
        let delay = (num as f64 / den as f64) as i32;
        let mut buffer = frame.buffer().clone();

        if let Some(f) = watermark_font.as_ref() {
            let mut dynamic = image::ImageBuffer::from_raw(w, h, buffer.as_raw().to_vec()).unwrap();
            apply_watermark(&mut dynamic, f);
            buffer = image::RgbaImage::from_raw(w, h, dynamic.into_raw()).unwrap();
        }
        encoder.add_frame(buffer.as_raw(), timestamp_ms)?;
        timestamp_ms += delay;
    }

    let output_path = Path::new("uploads").join(&new_name);
    let thumb_path = Path::new("uploads/thumbs").join(&thumb_name);
    let webp_data = encoder.finalize(timestamp_ms)?;

    let thumb = image::DynamicImage::ImageRgba8(frames[0].buffer().clone()).thumbnail(500, 500);
    let (tw, th) = (thumb.width(), thumb.height());
    let thumb_data = thumb.to_rgba8().into_raw();
    let thumb_enc = webp::Encoder::from_rgba(&thumb_data, tw, th);
    let encoded_thumb = thumb_enc.encode(70.0);

    let write_result: anyhow::Result<()> = (|| {
        fs::write(&output_path, webp_data)?;
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
}
