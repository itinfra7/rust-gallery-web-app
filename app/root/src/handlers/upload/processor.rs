use std::{fs, path::Path};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::state::AppState;
use super::animated::process_animated;
use super::static_img::process_static;

pub async fn process_image<F>(
    state: &AppState,
    temp_path: &Path,
    content_type: &str,
    update_status: F
) -> anyhow::Result<Option<(Uuid, DateTime<Utc>)>>
where F: Fn(&str)
{
    let final_filename = if content_type == "image/gif" {
        process_animated(temp_path, &update_status)?
    } else {
        process_static(temp_path, &update_status)?
    };

    if let Some(filename) = final_filename {
        update_status("Saving DB...");
        let image_id = Uuid::new_v4();
        let upload_date_result = sqlx::query_scalar::<_, DateTime<Utc>>(
            "INSERT INTO images (id, filename) VALUES ($1, $2) RETURNING upload_date"
        )
            .bind(image_id)
            .bind(&filename)
            .fetch_one(&state.db)
            .await;

        let upload_date = match upload_date_result {
            Ok(upload_date) => upload_date,
            Err(error) => {
                let output_path = Path::new("uploads").join(&filename);
                let thumb_name = filename.replace(".webp", "_thumb.webp");
                let thumb_path = Path::new("uploads/thumbs").join(thumb_name);

                if output_path.exists() {
                    fs::remove_file(&output_path).ok();
                }
                if thumb_path.exists() {
                    fs::remove_file(&thumb_path).ok();
                }

                return Err(error.into());
            }
        };
        return Ok(Some((image_id, upload_date)));
    }
    
    Ok(None)
}
