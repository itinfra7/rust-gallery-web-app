use axum::{
    extract::{Path as AxumPath, State},
    response::{IntoResponse, Response},
    http::{header, HeaderMap, HeaderValue, StatusCode},
};
use chrono::{DateTime, Utc};
use crate::state::AppState;
use crate::handlers::common::{apply_cache_headers, build_weak_etag, request_not_modified};
use super::models::{SitemapImage, SitemapTag};
use super::builder::{build_sitemap_xml, build_rss_xml, build_tag_rss_xml};

fn build_xml_response(
    body: String,
    request_headers: &HeaderMap,
    last_modified: Option<DateTime<Utc>>,
) -> Response {
    let etag = build_weak_etag(&body);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/xml; charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=0, must-revalidate"),
    );
    apply_cache_headers(&mut headers, &etag, last_modified);

    if request_not_modified(request_headers, &etag, last_modified) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }

    (headers, body).into_response()
}

async fn find_tag_name_by_slug(state: &AppState, slug: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        r#"
        SELECT tag
        FROM (
            SELECT DISTINCT unnest(tags) AS tag
            FROM images
            WHERE tags IS NOT NULL
        ) tag_pool
        WHERE trim(both '-' from lower(regexp_replace(tag, '[^A-Za-z0-9]+', '-', 'g'))) = $1
        ORDER BY length(tag), tag
        LIMIT 1
        "#
    )
    .bind(slug)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
}

pub async fn robots_txt_handler() -> impl IntoResponse {
    let robots = r#"User-agent: *
Allow: /
Sitemap: https://<PUBLIC_DOMAIN>/sitemap.xml
"#;
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        robots,
    )
}

pub async fn sitemap_xml_handler(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> impl IntoResponse {
    let images = sqlx::query_as::<_, SitemapImage>(
        r#"
        SELECT id, filename, tags, upload_date
        FROM images
        ORDER BY upload_date DESC
        LIMIT 5000
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let tags = sqlx::query_as::<_, SitemapTag>(
        r#"
        SELECT tag, upload_date, image_id, image_tags
        FROM (
            SELECT DISTINCT ON (tag)
                tag,
                i.upload_date,
                i.id AS image_id,
                i.tags AS image_tags
            FROM images i
            CROSS JOIN LATERAL unnest(i.tags) AS tag
            ORDER BY tag, i.upload_date DESC
        ) tag_index
        ORDER BY upload_date DESC
        LIMIT 5000
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let last_modified = images
        .first()
        .map(|image| image.upload_date)
        .into_iter()
        .chain(tags.first().map(|tag| tag.upload_date))
        .max();
    let sitemap = build_sitemap_xml(images, tags);

    build_xml_response(sitemap, &request_headers, last_modified)
}

pub async fn rss_xml_handler(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> impl IntoResponse {
    let images = sqlx::query_as::<_, SitemapImage>(
        r#"
        SELECT id, filename, tags, upload_date
        FROM images
        ORDER BY upload_date DESC
        LIMIT 50
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let last_modified = images.first().map(|image| image.upload_date);
    let rss = build_rss_xml(images);

    build_xml_response(rss, &request_headers, last_modified)
}

pub async fn tag_rss_xml_handler(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> impl IntoResponse {
    let Some(tag_name) = find_tag_name_by_slug(&state, &slug).await else {
        return (StatusCode::NOT_FOUND, "Tag not found").into_response();
    };

    let images = sqlx::query_as::<_, SitemapImage>(
        r#"
        SELECT id, filename, tags, upload_date
        FROM images
        WHERE $1 = ANY(tags)
        ORDER BY upload_date DESC
        LIMIT 50
        "#
    )
    .bind(&tag_name)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if images.is_empty() {
        return (StatusCode::NOT_FOUND, "Tag not found").into_response();
    }

    let last_modified = images.first().map(|image| image.upload_date);
    let rss = build_tag_rss_xml(&tag_name, images);

    build_xml_response(rss, &request_headers, last_modified)
}
