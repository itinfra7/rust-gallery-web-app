use askama::Template as AskamaTemplate;
use axum::{
    extract::{State, Query, ConnectInfo, Path as AxumPath},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Redirect, Response},
};
use axum_extra::extract::cookie::CookieJar;
use image::image_dimensions;
use serde_json::json;
use tokio::fs;
use std::net::SocketAddr;
use sqlx::{Row, QueryBuilder, Postgres};
use tracing::error;
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::alerting::spawn_runtime_alert;
use crate::state::AppState;
use crate::auth::check_session;
use super::common::{
    apply_cache_headers, build_image_page_path, build_image_slug, build_media_path,
    build_tag_path, build_thumb_path, build_weak_etag, make_absolute_url, request_not_modified,
    summarize_tags, GalleryTemplate, ImageEntry, ImagePageTemplate, ImageQuery, TagLink,
    TagPageTemplate, get_client_ip,
};
use super::query::build_search_query;

struct LoadedImagePage {
    id: Uuid,
    file_name: String,
    tags: Vec<String>,
    upload_date: DateTime<Utc>,
    like_count: i32,
    comment_count: i64,
}

fn build_image_metadata(tags: &[String], upload_date: &str) -> (String, String, String) {
    let short_summary = summarize_tags(tags, 4);
    let long_summary = summarize_tags(tags, 8);

    let page_title = if short_summary.is_empty() {
        "Image | <PUBLIC_DOMAIN>".to_string()
    } else {
        format!("{} | <PUBLIC_DOMAIN>", short_summary)
    };

    let meta_description = if long_summary.is_empty() {
        format!("View a curated image on <PUBLIC_DOMAIN> uploaded on {}.", upload_date)
    } else {
        format!(
            "View a curated <PUBLIC_DOMAIN> image tagged {}. Uploaded on {}.",
            long_summary,
            upload_date
        )
    };

    let image_alt = if long_summary.is_empty() {
        format!("Curated image on <PUBLIC_DOMAIN> uploaded on {}", upload_date)
    } else {
        format!("Image tagged {}", long_summary)
    };

    (page_title, meta_description, image_alt)
}

fn build_tag_page_metadata(tag_name: &str, image_count: i64) -> (String, String, String) {
    let page_title = format!("{} images | <PUBLIC_DOMAIN>", tag_name);
    let meta_description = format!(
        "Browse {} images tagged {} on <PUBLIC_DOMAIN>.",
        image_count.max(1),
        tag_name
    );
    let og_image_alt = format!("<PUBLIC_DOMAIN> images tagged {}", tag_name);

    (page_title, meta_description, og_image_alt)
}

async fn read_image_dimensions(path: String) -> Option<(u32, u32)> {
    tokio::task::spawn_blocking(move || image_dimensions(path).ok())
        .await
        .ok()
        .flatten()
}

fn serialize_json_ld(value: serde_json::Value) -> String {
    serde_json::to_string(&value)
        .unwrap_or_else(|_| "{}".to_string())
        .replace("</", "<\\/")
}

fn render_html_response<T: AskamaTemplate>(
    template: T,
    request_headers: &HeaderMap,
    last_modified: Option<DateTime<Utc>>,
    cache_control: &'static str,
) -> Response {
    let body = match template.render() {
        Ok(body) => body,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let etag = build_weak_etag(&body);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_control));
    apply_cache_headers(&mut headers, &etag, last_modified);

    if request_not_modified(request_headers, &etag, last_modified) {
        return (StatusCode::NOT_MODIFIED, headers).into_response();
    }

    (headers, body).into_response()
}

fn build_home_structured_data(images: &[ImageEntry], og_image_url: &str) -> String {
    let item_list = images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let item_name = if image.tags.is_empty() {
                format!("Image uploaded on {} | <PUBLIC_DOMAIN>", image.date)
            } else {
                format!("{} | <PUBLIC_DOMAIN>", summarize_tags(&image.tags, 4))
            };

            json!({
                "@type": "ListItem",
                "position": index + 1,
                "url": make_absolute_url(&image.page_url),
                "name": item_name,
                "image": make_absolute_url(&image.media_url),
            })
        })
        .collect::<Vec<_>>();

    serialize_json_ld(json!({
        "@context": "https://schema.org",
        "@type": "CollectionPage",
        "url": make_absolute_url("/"),
        "name": "<PUBLIC_DOMAIN>",
        "description": "Explore the <PUBLIC_DOMAIN> image collection.",
        "image": og_image_url,
        "mainEntity": {
            "@type": "ItemList",
            "name": "Latest <PUBLIC_DOMAIN> gallery images",
            "numberOfItems": item_list.len(),
            "itemListOrder": "https://schema.org/ItemListOrderDescending",
            "itemListElement": item_list,
        }
    }))
}

fn build_tag_page_structured_data(
    tag_name: &str,
    canonical_url: &str,
    meta_description: &str,
    og_image_url: &str,
    og_image_alt: &str,
    images: &[ImageEntry],
    dimensions: Option<(u32, u32)>,
) -> String {
    let item_list = images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            json!({
                "@type": "ListItem",
                "position": index + 1,
                "url": make_absolute_url(&image.page_url),
                "name": if image.tags.is_empty() {
                    format!("Image uploaded on {} | <PUBLIC_DOMAIN>", image.date)
                } else {
                    format!("{} | <PUBLIC_DOMAIN>", summarize_tags(&image.tags, 4))
                },
                "image": make_absolute_url(&image.media_url),
            })
        })
        .collect::<Vec<_>>();

    let mut image_object = json!({
        "@type": "ImageObject",
        "contentUrl": og_image_url,
        "caption": og_image_alt,
        "encodingFormat": "image/webp",
    });

    if let Some((width, height)) = dimensions {
        image_object["width"] = json!(width);
        image_object["height"] = json!(height);
    }

    serialize_json_ld(json!({
        "@context": "https://schema.org",
        "@type": "CollectionPage",
        "url": canonical_url,
        "name": format!("{} images | <PUBLIC_DOMAIN>", tag_name),
        "description": meta_description,
        "image": image_object,
        "about": {
            "@type": "Thing",
            "name": tag_name,
        },
        "mainEntity": {
            "@type": "ItemList",
            "name": format!("<PUBLIC_DOMAIN> images tagged {}", tag_name),
            "numberOfItems": item_list.len(),
            "itemListOrder": "https://schema.org/ItemListOrderDescending",
            "itemListElement": item_list,
        }
    }))
}

fn build_image_structured_data(
    canonical_url: &str,
    og_image_url: &str,
    thumb_url: &str,
    page_title: &str,
    meta_description: &str,
    image_alt: &str,
    upload_date_iso: &str,
    tags: &[String],
    dimensions: Option<(u32, u32)>,
) -> String {
    let mut object = json!({
        "@context": "https://schema.org",
        "@type": "ImageObject",
        "name": page_title,
        "description": meta_description,
        "url": canonical_url,
        "contentUrl": og_image_url,
        "thumbnailUrl": thumb_url,
        "caption": image_alt,
        "uploadDate": upload_date_iso,
        "keywords": tags,
        "encodingFormat": "image/webp",
        "representativeOfPage": true,
        "mainEntityOfPage": canonical_url,
        "isPartOf": {
            "@type": "CollectionPage",
            "url": make_absolute_url("/"),
            "name": "<PUBLIC_DOMAIN>",
        }
    });

    if let Some((width, height)) = dimensions {
        object["width"] = json!(width);
        object["height"] = json!(height);
    }

    serialize_json_ld(object)
}

fn build_breadcrumb_structured_data(canonical_url: &str, item_name: &str) -> String {
    serialize_json_ld(json!({
        "@context": "https://schema.org",
        "@type": "BreadcrumbList",
        "itemListElement": [
            {
                "@type": "ListItem",
                "position": 1,
                "name": "<PUBLIC_DOMAIN>",
                "item": make_absolute_url("/"),
            },
            {
                "@type": "ListItem",
                "position": 2,
                "name": item_name,
                "item": canonical_url,
            }
        ]
    }))
}

fn build_image_entry(
    id: Uuid,
    file_name: String,
    tags: Vec<String>,
    date: DateTime<Utc>,
    like_count: i32,
    comment_count: i64,
    is_liked: bool,
) -> ImageEntry {
    let date_string = date.format("%Y-%m-%d").to_string();
    let (_, _, alt_text) = build_image_metadata(&tags, &date_string);
    let id_string = id.to_string();

    ImageEntry {
        id: id_string.clone(),
        page_url: build_image_page_path(&id_string, &tags, &date_string),
        media_url: build_media_path(&id_string, &tags, &date_string),
        thumb_url: build_thumb_path(&id_string, &tags, &date_string),
        thumb: file_name.replace(".webp", "_thumb.webp"),
        alt_text,
        is_animated: file_name.contains("_anim"),
        name: file_name,
        tags,
        date: date_string,
        like_count,
        comment_count,
        is_liked,
    }
}

pub async fn index_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    jar: CookieJar,
) -> impl IntoResponse {
    let is_admin = check_session(&state, &jar);

    let rows = sqlx::query(
        r#"
        SELECT
            id, filename, tags, upload_date, like_count,
            (SELECT COUNT(*) FROM comments c WHERE c.image_id = images.id) as comment_count,
            FALSE as is_liked,
            COUNT(*) OVER() as total_count
        FROM images
        ORDER BY upload_date DESC
        LIMIT 24
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut images = Vec::new();
    let mut total_count: i64 = 0;
    let last_modified = rows
        .first()
        .map(|row| row.get::<DateTime<Utc>, _>("upload_date"));
    if !rows.is_empty() {
        total_count = rows[0].get("total_count");
    }

    for row in rows {
        let id: Uuid = row.get("id");
        let file_name: String = row.get("filename");
        let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
        let date: DateTime<Utc> = row.get("upload_date");
        let like_count: i32 = row.get("like_count");
        let comment_count: i64 = row.get("comment_count");
        let is_liked: bool = row.get("is_liked");

        images.push(build_image_entry(
            id,
            file_name,
            tags,
            date,
            like_count,
            comment_count,
            is_liked,
        ));
    }

    let og_image_url = images
        .first()
        .map(|image| make_absolute_url(&image.media_url))
        .unwrap_or_else(|| make_absolute_url("/public/icons/apple-touch-icon.png"));
    let og_image_alt = images
        .first()
        .map(|image| image.alt_text.clone())
        .unwrap_or_else(|| "<PUBLIC_DOMAIN> gallery preview".to_string());
    let og_image_dimensions = match images.first() {
        Some(image) => read_image_dimensions(format!("uploads/{}", image.name)).await,
        None => None,
    };
    let structured_data_json = build_home_structured_data(&images, &og_image_url);

    let response = render_html_response(
        GalleryTemplate {
            images,
            is_admin,
            total_count,
            og_image_url,
            og_image_alt,
            has_og_image_dimensions: og_image_dimensions.is_some(),
            og_image_width: og_image_dimensions.map(|(width, _)| width).unwrap_or(0),
            og_image_height: og_image_dimensions.map(|(_, height)| height).unwrap_or(0),
            structured_data_json,
        },
        &headers,
        last_modified,
        "private, max-age=0, must-revalidate",
    );

    let mut response = response;
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Cookie"));

    (jar, response)
}

async fn load_image_page(state: &AppState, id: Uuid) -> Result<Option<LoadedImagePage>, StatusCode> {
    match sqlx::query(
        r#"
        SELECT
            id, filename, tags, upload_date, like_count,
            (SELECT COUNT(*) FROM comments c WHERE c.image_id = images.id) as comment_count
        FROM images
        WHERE id = $1
        LIMIT 1
        "#
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(row)) => Ok(Some(LoadedImagePage {
            id: row.get("id"),
            file_name: row.get("filename"),
            tags: row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default(),
            upload_date: row.get("upload_date"),
            like_count: row.get("like_count"),
            comment_count: row.get("comment_count"),
        })),
        Ok(None) => Ok(None),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

pub async fn legacy_image_page_handler(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> impl IntoResponse {
    let image = match load_image_page(&state, id).await {
        Ok(Some(image)) => image,
        Ok(None) => return (StatusCode::NOT_FOUND, "Image not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load image page",
            )
                .into_response()
        }
    };

    let upload_date_str = image.upload_date.format("%Y-%m-%d").to_string();
    let canonical_path = build_image_page_path(&image.id.to_string(), &image.tags, &upload_date_str);

    Redirect::permanent(&canonical_path).into_response()
}

pub async fn image_page_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath((id, slug)): AxumPath<(Uuid, String)>,
) -> impl IntoResponse {
    let image = match load_image_page(&state, id).await {
        Ok(Some(image)) => image,
        Ok(None) => return (StatusCode::NOT_FOUND, "Image not found").into_response(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load image page",
            )
                .into_response()
        }
    };

    let image_id = image.id;
    let file_name = image.file_name;
    let tags = image.tags;
    let upload_date = image.upload_date;
    let like_count = image.like_count;
    let comment_count = image.comment_count;
    let upload_date_str = upload_date.format("%Y-%m-%d").to_string();
    let upload_date_iso = upload_date.to_rfc3339();
    let is_animated = file_name.contains("_anim");
    let (page_title, meta_description, image_alt) = build_image_metadata(&tags, &upload_date_str);
    let image_id_string = image_id.to_string();
    let expected_slug = build_image_slug(&tags, &upload_date_str);
    let canonical_path = build_image_page_path(&image_id_string, &tags, &upload_date_str);
    if slug != expected_slug {
        return Redirect::permanent(&canonical_path).into_response();
    }
    let canonical_url = make_absolute_url(&canonical_path);
    let image_url = build_media_path(&image_id_string, &tags, &upload_date_str);
    let thumb_url = build_thumb_path(&image_id_string, &tags, &upload_date_str);
    let og_image_url = make_absolute_url(&image_url);
    let og_thumb_url = make_absolute_url(&thumb_url);
    let image_dimensions = read_image_dimensions(format!("uploads/{}", file_name)).await;
    let breadcrumb_label = {
        let summary = summarize_tags(&tags, 4);
        if summary.is_empty() {
            page_title.clone()
        } else {
            summary
        }
    };
    let structured_data_json = build_image_structured_data(
        &canonical_url,
        &og_image_url,
        &og_thumb_url,
        &page_title,
        &meta_description,
        &image_alt,
        &upload_date_iso,
        &tags,
        image_dimensions,
    );
    let breadcrumb_json = build_breadcrumb_structured_data(&canonical_url, &page_title);
    let tag_links = tags
        .iter()
        .map(|tag| TagLink {
            name: tag.clone(),
            url: build_tag_path(tag),
        })
        .collect::<Vec<_>>();

    render_html_response(
        ImagePageTemplate {
            page_title,
            breadcrumb_label,
            meta_description,
            canonical_url,
            og_image_url,
            image_url,
            image_alt,
            image_id: image_id_string,
            upload_date: upload_date_str,
            like_count,
            comment_count,
            is_animated,
            has_image_dimensions: image_dimensions.is_some(),
            image_width: image_dimensions.map(|(width, _)| width).unwrap_or(0),
            image_height: image_dimensions.map(|(_, height)| height).unwrap_or(0),
            tag_links,
            structured_data_json,
            breadcrumb_json,
        },
        &headers,
        Some(upload_date),
        "public, max-age=0, must-revalidate",
    )
}

async fn find_tag_name_by_slug(state: &AppState, slug: &str) -> Result<Option<String>, StatusCode> {
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
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn tag_page_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(slug): AxumPath<String>,
) -> impl IntoResponse {
    let tag_name = match find_tag_name_by_slug(&state, &slug).await {
        Ok(Some(tag_name)) => tag_name,
        Ok(None) => return (StatusCode::NOT_FOUND, "Tag not found").into_response(),
        Err(status) => return status.into_response(),
    };

    let rows = match sqlx::query(
        r#"
        SELECT
            id, filename, tags, upload_date, like_count,
            (SELECT COUNT(*) FROM comments c WHERE c.image_id = images.id) as comment_count,
            FALSE as is_liked,
            COUNT(*) OVER() as total_count
        FROM images
        WHERE $1 = ANY(tags)
        ORDER BY upload_date DESC
        LIMIT 96
        "#
    )
    .bind(&tag_name)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if rows.is_empty() {
        return (StatusCode::NOT_FOUND, "Tag not found").into_response();
    }

    let mut images = Vec::new();
    let total_count: i64 = rows[0].get("total_count");
    let last_modified = rows
        .first()
        .map(|row| row.get::<DateTime<Utc>, _>("upload_date"));

    for row in rows {
        let id: Uuid = row.get("id");
        let file_name: String = row.get("filename");
        let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
        let date: DateTime<Utc> = row.get("upload_date");
        let like_count: i32 = row.get("like_count");
        let comment_count: i64 = row.get("comment_count");
        let is_liked: bool = row.get("is_liked");

        images.push(build_image_entry(
            id,
            file_name,
            tags,
            date,
            like_count,
            comment_count,
            is_liked,
        ));
    }

    let canonical_url = make_absolute_url(&format!("/tag/{}", slug));
    let rss_url = make_absolute_url(&format!("/tag/{}/rss.xml", slug));
    let (page_title, meta_description, og_image_alt) =
        build_tag_page_metadata(&tag_name, total_count);
    let og_image_url = images
        .first()
        .map(|image| make_absolute_url(&image.media_url))
        .unwrap_or_else(|| make_absolute_url("/public/icons/apple-touch-icon.png"));
    let og_image_dimensions = match images.first() {
        Some(image) => read_image_dimensions(format!("uploads/{}", image.name)).await,
        None => None,
    };
    let structured_data_json = build_tag_page_structured_data(
        &tag_name,
        &canonical_url,
        &meta_description,
        &og_image_url,
        &og_image_alt,
        &images,
        og_image_dimensions,
    );
    let breadcrumb_json = build_breadcrumb_structured_data(&canonical_url, &tag_name);

    render_html_response(
        TagPageTemplate {
            tag_name,
            page_title,
            meta_description,
            canonical_url,
            rss_url,
            og_image_url,
            og_image_alt,
            has_og_image_dimensions: og_image_dimensions.is_some(),
            og_image_width: og_image_dimensions.map(|(width, _)| width).unwrap_or(0),
            og_image_height: og_image_dimensions.map(|(_, height)| height).unwrap_or(0),
            total_count,
            images,
            structured_data_json,
            breadcrumb_json,
        },
        &headers,
        last_modified,
        "public, max-age=0, must-revalidate",
    )
}

pub async fn get_images_api_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ImageQuery>,
) -> impl IntoResponse {
    let client_ip = get_client_ip(&headers, Some(addr));

    let builder: QueryBuilder<Postgres> = QueryBuilder::new(
        r#"
        SELECT
            i.id, i.filename, i.tags, i.upload_date, i.like_count,
            (SELECT COUNT(*) FROM comments c WHERE c.image_id = i.id) as comment_count,
            CASE WHEN l.user_id IS NOT NULL THEN TRUE ELSE FALSE END as is_liked,
            COUNT(*) OVER() as total_count
        FROM images i
        "#
    );

    let mut query = build_search_query(builder, &params, &client_ip);
    let rows = match query.build().fetch_all(&state.db).await {
        Ok(rows) => rows,
        Err(err) => {
            error!(
                "Failed to fetch /api/images results for page={:?}, limit={:?}, sort={:?}, tag={:?}, search={:?}, fingerprint_present={}: {}",
                params.page,
                params.limit,
                params.sort,
                params.tag,
                params.search,
                params.fingerprint.is_some(),
                err
            );
            spawn_runtime_alert(
                "error",
                "api-images",
                "api_images_query_failed",
                "API images query failed",
                format!(
                    "page={:?} limit={:?} sort={:?} tag={:?} search={:?} fingerprint_present={} error={}",
                    params.page,
                    params.limit,
                    params.sort,
                    params.tag,
                    params.search,
                    params.fingerprint.is_some(),
                    err
                ),
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let mut images = Vec::new();
    let mut total_count: i64 = 0;

    if !rows.is_empty() {
        total_count = rows[0].get("total_count");
    }

    for row in rows {
        let id: Uuid = row.get("id");
        let file_name: String = row.get("filename");
        let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
        let date: DateTime<Utc> = row.get("upload_date");
        let like_count: i32 = row.get("like_count");
        let comment_count: i64 = row.get("comment_count");
        let is_liked: bool = row.get("is_liked");

        images.push(build_image_entry(
            id,
            file_name,
            tags,
            date,
            like_count,
            comment_count,
            is_liked,
        ));
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert("X-Total-Count", total_count.into());

    (response_headers, Json(images)).into_response()
}

async fn find_image_filename(state: &AppState, id: Uuid) -> Result<String, StatusCode> {
    match sqlx::query_scalar::<_, String>(
        r#"
        SELECT filename
        FROM images
        WHERE id = $1
        LIMIT 1
        "#
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(filename)) => Ok(filename),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn serve_webp_file(path: String) -> axum::response::Response {
    match fs::read(&path).await {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
            headers.insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=31536000, immutable"),
            );
            headers.insert(
                HeaderName::from_static("x-robots-tag"),
                HeaderValue::from_static("max-image-preview:large"),
            );
            (headers, bytes).into_response()
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn media_file_handler(
    State(state): State<AppState>,
    AxumPath((id, _slug)): AxumPath<(Uuid, String)>,
) -> impl IntoResponse {
    match find_image_filename(&state, id).await {
        Ok(filename) => serve_webp_file(format!("uploads/{}", filename)).await,
        Err(status) => status.into_response(),
    }
}

pub async fn thumb_file_handler(
    State(state): State<AppState>,
    AxumPath((id, _slug)): AxumPath<(Uuid, String)>,
) -> impl IntoResponse {
    match find_image_filename(&state, id).await {
        Ok(filename) => {
            let thumb_name = filename.replace(".webp", "_thumb.webp");
            serve_webp_file(format!("uploads/thumbs/{}", thumb_name)).await
        }
        Err(status) => status.into_response(),
    }
}

pub async fn get_related_images_handler(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<Uuid>,
) -> impl IntoResponse {
    let rows = sqlx::query(
        r#"
        SELECT
            i.id, i.filename, i.tags, i.upload_date, i.like_count,
            (SELECT COUNT(*) FROM comments c WHERE c.image_id = i.id) as comment_count,
            FALSE as is_liked
        FROM images i
        WHERE i.id != $1
        AND i.tags && (SELECT tags FROM images WHERE id = $1)
        ORDER BY RANDOM()
        LIMIT 6
        "#
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut images = Vec::new();
    for row in rows {
        let id: Uuid = row.get("id");
        let file_name: String = row.get("filename");
        let tags: Vec<String> = row.get::<Option<Vec<String>>, _>("tags").unwrap_or_default();
        let date: DateTime<Utc> = row.get("upload_date");
        let like_count: i32 = row.get("like_count");
        let comment_count: i64 = row.get("comment_count");
        let is_liked: bool = row.get("is_liked");

        images.push(build_image_entry(
            id,
            file_name,
            tags,
            date,
            like_count,
            comment_count,
            is_liked,
        ));
    }

    Json(images)
}
