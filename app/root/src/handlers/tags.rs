use axum::{
    extract::State,
    response::{IntoResponse, Json},
};
use crate::state::AppState;
use sqlx::Row;

pub async fn get_all_tags_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query(
        "SELECT DISTINCT UNNEST(tags) as tag FROM images ORDER BY tag"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let tags: Vec<String> = rows
        .into_iter()
        .map(|row| row.get("tag"))
        .collect();

    Json(tags)
}
