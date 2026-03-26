use axum::{
    extract::State,
    response::{IntoResponse, Json},
};
use crate::state::AppState;
use sqlx::Row;

pub async fn get_search_suggestions_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT UNNEST(tags) AS text
        FROM images
        ORDER BY text ASC
        LIMIT 1000
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let suggestions: Vec<String> = rows
        .into_iter()
        .map(|row| row.get::<String, _>("text"))
        .collect();

    Json(suggestions)
}
