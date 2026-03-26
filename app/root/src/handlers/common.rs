use serde::{Deserialize, Serialize};
use askama::Template;
use axum::http::{header, HeaderMap, HeaderValue};
use chrono::{DateTime, Utc};
use httpdate::{fmt_http_date, parse_http_date};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const BASE_URL: &str = "https://<PUBLIC_DOMAIN>";

#[derive(Serialize, Clone)]
pub struct ImageEntry {
    pub id: String,
    pub page_url: String,
    pub media_url: String,
    pub thumb_url: String,
    pub name: String,
    pub thumb: String,
    pub alt_text: String,
    pub is_animated: bool,
    pub tags: Vec<String>,
    pub date: String,
    pub like_count: i32,
    pub comment_count: i64,
    pub is_liked: bool,
}

#[derive(Clone)]
pub struct TagLink {
    pub name: String,
    pub url: String,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct GalleryTemplate {
    pub images: Vec<ImageEntry>,
    pub is_admin: bool,
    pub total_count: i64,
    pub og_image_url: String,
    pub og_image_alt: String,
    pub has_og_image_dimensions: bool,
    pub og_image_width: u32,
    pub og_image_height: u32,
    pub structured_data_json: String,
}

#[derive(Template)]
#[template(path = "tag.html")]
pub struct TagPageTemplate {
    pub tag_name: String,
    pub page_title: String,
    pub meta_description: String,
    pub canonical_url: String,
    pub rss_url: String,
    pub og_image_url: String,
    pub og_image_alt: String,
    pub has_og_image_dimensions: bool,
    pub og_image_width: u32,
    pub og_image_height: u32,
    pub total_count: i64,
    pub images: Vec<ImageEntry>,
    pub structured_data_json: String,
    pub breadcrumb_json: String,
}

#[derive(Template)]
#[template(path = "image.html")]
pub struct ImagePageTemplate {
    pub page_title: String,
    pub breadcrumb_label: String,
    pub meta_description: String,
    pub canonical_url: String,
    pub og_image_url: String,
    pub image_url: String,
    pub image_alt: String,
    pub image_id: String,
    pub upload_date: String,
    pub like_count: i32,
    pub comment_count: i64,
    pub is_animated: bool,
    pub has_image_dimensions: bool,
    pub image_width: u32,
    pub image_height: u32,
    pub tag_links: Vec<TagLink>,
    pub structured_data_json: String,
    pub breadcrumb_json: String,
}

#[derive(Deserialize)]
pub struct LoginPayload {
    pub id: String,
    pub password: String,
    pub key: String,
}

#[derive(Deserialize)]
pub struct BulkDeletePayload {
    pub filenames: Vec<String>,
}

#[derive(Deserialize)]
pub struct ImageQuery {
    pub sort: Option<String>,
    pub tag: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub search: Option<String>,
    pub fingerprint: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateTagsPayload {
    pub tags: String,
}

pub fn get_client_ip(headers: &HeaderMap, addr: Option<SocketAddr>) -> String {
    if let Some(ip) = headers.get("X-Real-IP") {
        if let Ok(s) = ip.to_str() {
            return s.to_string();
        }
    }
    if let Some(ip) = headers.get("X-Forwarded-For") {
        if let Ok(s) = ip.to_str() {
            return s.split(',').next().unwrap_or("").trim().to_string();
        }
    }
    if let Some(addr) = addr {
        return addr.ip().to_string();
    }
    "0.0.0.0".to_string()
}

pub fn build_rate_limit_key(scope: &str, client_ip: &str, fingerprint: Option<&str>) -> String {
    let mut hasher = DefaultHasher::new();
    scope.hash(&mut hasher);
    client_ip.trim().hash(&mut hasher);
    fingerprint.unwrap_or("").trim().hash(&mut hasher);
    format!("{}:{:016x}", scope, hasher.finish())
}

pub fn retry_after_headers(retry_after_seconds: u64) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
        headers.insert(header::RETRY_AFTER, value);
    }
    headers
}

pub fn summarize_tags(tags: &[String], limit: usize) -> String {
    tags.iter()
        .take(limit)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalize_slug_part(input: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !slug.is_empty() && !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }

    slug.trim_matches('-').to_string()
}

pub fn build_tag_slug(tag: &str) -> String {
    let slug = normalize_slug_part(tag);
    if slug.is_empty() {
        "tag".to_string()
    } else {
        slug
    }
}

pub fn build_image_slug(tags: &[String], upload_date: &str) -> String {
    let mut parts = Vec::new();

    for tag in tags.iter().take(4) {
        let normalized = normalize_slug_part(tag);
        if !normalized.is_empty() {
            parts.push(normalized);
        }
    }

    let mut slug = if parts.is_empty() {
        let date_slug = normalize_slug_part(upload_date);
        if date_slug.is_empty() {
            "image".to_string()
        } else {
            format!("image-{}", date_slug)
        }
    } else {
        parts.join("-")
    };

    if slug.len() > 80 {
        slug.truncate(80);
        slug = slug.trim_matches('-').to_string();
    }

    if slug.is_empty() {
        "image".to_string()
    } else {
        slug
    }
}

pub fn build_image_page_path(image_id: &str, tags: &[String], upload_date: &str) -> String {
    format!("/image/{}/{}", image_id, build_image_slug(tags, upload_date))
}

pub fn build_media_path(image_id: &str, tags: &[String], upload_date: &str) -> String {
    format!("/media/{}/{}.webp", image_id, build_image_slug(tags, upload_date))
}

pub fn build_thumb_path(image_id: &str, tags: &[String], upload_date: &str) -> String {
    format!("/thumbs/{}/{}.webp", image_id, build_image_slug(tags, upload_date))
}

pub fn build_tag_path(tag: &str) -> String {
    format!("/tag/{}", build_tag_slug(tag))
}

pub fn make_absolute_url(path: &str) -> String {
    format!("{}{}", BASE_URL, path)
}

pub fn build_weak_etag(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("W/\"{:016x}\"", hasher.finish())
}

fn to_system_time(last_modified: DateTime<Utc>) -> Option<SystemTime> {
    let seconds = u64::try_from(last_modified.timestamp()).ok()?;
    Some(UNIX_EPOCH + Duration::from_secs(seconds))
}

pub fn apply_cache_headers(
    headers: &mut HeaderMap,
    etag: &str,
    last_modified: Option<DateTime<Utc>>,
) {
    if let Ok(value) = HeaderValue::from_str(etag) {
        headers.insert(header::ETAG, value);
    }

    if let Some(last_modified) = last_modified
        .and_then(to_system_time)
        .map(fmt_http_date)
        .and_then(|value| HeaderValue::from_str(&value).ok())
    {
        headers.insert(header::LAST_MODIFIED, last_modified);
    }
}

pub fn request_not_modified(
    request_headers: &HeaderMap,
    etag: &str,
    last_modified: Option<DateTime<Utc>>,
) -> bool {
    if let Some(if_none_match) = request_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    {
        for candidate in if_none_match.split(',') {
            let candidate = candidate.trim();
            if candidate == "*" || candidate == etag {
                return true;
            }
        }

        return false;
    }

    let Some(last_modified) = last_modified.and_then(to_system_time) else {
        return false;
    };

    let Some(if_modified_since) = request_headers
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };

    match parse_http_date(if_modified_since) {
        Ok(parsed) => parsed >= last_modified,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, HeaderMap, HeaderValue};
    use chrono::TimeZone;
    use std::net::{Ipv4Addr, SocketAddr};

    #[test]
    fn build_image_slug_prefers_tag_names() {
        let tags = vec![
            "Skirt".to_string(),
            "Sleeveless Top".to_string(),
            "Stockings".to_string(),
        ];

        assert_eq!(
            build_image_slug(&tags, "2026-01-25"),
            "skirt-sleeveless-top-stockings"
        );
        assert_eq!(
            build_image_page_path("abc", &tags, "2026-01-25"),
            "/image/abc/skirt-sleeveless-top-stockings"
        );
        assert_eq!(
            build_media_path("abc", &tags, "2026-01-25"),
            "/media/abc/skirt-sleeveless-top-stockings.webp"
        );
        assert_eq!(
            build_thumb_path("abc", &tags, "2026-01-25"),
            "/thumbs/abc/skirt-sleeveless-top-stockings.webp"
        );
    }

    #[test]
    fn build_image_slug_falls_back_to_date_or_image() {
        let empty_tags = Vec::<String>::new();
        assert_eq!(build_image_slug(&empty_tags, "2026-01-25"), "image-2026-01-25");
        assert_eq!(build_image_slug(&empty_tags, "???"), "image");
    }

    #[test]
    fn build_tag_slug_has_safe_fallback() {
        assert_eq!(build_tag_slug("Sleeveless Top"), "sleeveless-top");
        assert_eq!(build_tag_slug("!!!"), "tag");
        assert_eq!(build_tag_path("Animal Ears"), "/tag/animal-ears");
    }

    #[test]
    fn request_not_modified_honors_if_none_match_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("W/\"wrong\""));
        headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static("Fri, 21 Mar 2031 00:00:00 GMT"),
        );
        let last_modified = Some(Utc.with_ymd_and_hms(2026, 3, 21, 0, 0, 0).unwrap());

        assert!(!request_not_modified(&headers, "W/\"right\"", last_modified));
    }

    #[test]
    fn request_not_modified_uses_if_modified_since_without_etag() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_static("Sat, 21 Mar 2026 00:00:00 GMT"),
        );
        let last_modified = Some(Utc.with_ymd_and_hms(2026, 3, 21, 0, 0, 0).unwrap());

        assert!(request_not_modified(&headers, "W/\"etag\"", last_modified));
    }

    #[test]
    fn get_client_ip_prefers_proxy_headers() {
        let socket = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), 8080));
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Forwarded-For",
            HeaderValue::from_static("203.0.113.9, 10.0.0.1"),
        );
        assert_eq!(get_client_ip(&headers, Some(socket)), "203.0.113.9");

        headers.insert("X-Real-IP", HeaderValue::from_static("198.51.100.2"));
        assert_eq!(get_client_ip(&headers, Some(socket)), "198.51.100.2");
    }
}
