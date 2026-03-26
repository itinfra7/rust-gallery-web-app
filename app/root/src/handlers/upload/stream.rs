use axum::extract::multipart::Field;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use std::path::Path;

pub async fn save_multipart_to_temp(mut field: Field<'_>, path: &Path) -> anyhow::Result<()> {
    let mut file = File::create(path).await?;
    
    while let Some(chunk) = field.chunk().await? {
        file.write_all(&chunk).await?;
    }
    
    file.flush().await?;
    Ok(())
}
