use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub struct SitemapImage {
    pub id: Uuid,
    pub filename: String,
    pub tags: Option<Vec<String>>,
    pub upload_date: DateTime<Utc>,
}

#[derive(FromRow)]
pub struct SitemapTag {
    pub tag: String,
    pub upload_date: DateTime<Utc>,
    pub image_id: Uuid,
    pub image_tags: Option<Vec<String>>,
}
