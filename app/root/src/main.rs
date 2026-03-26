mod state;
mod auth;
mod audit;
mod alerting;
mod rate_limit;
mod utils;
mod handlers;
#[cfg(test)]
mod test_support;

use axum::{
    extract::DefaultBodyLimit,
    http::{header, HeaderValue},
    middleware,
    routing::{get, post, delete},
    Router,
};
use tower_http::{
    services::ServeDir,
    set_header::SetResponseHeaderLayer,
};
use std::{fs, net::SocketAddr, env};
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;

use crate::state::AppState;
use crate::auth::{init as auth_init, spawn_session_cleaner};
use crate::handlers::{
    index_handler, login_handler, logout_handler,
    upload_handler, delete_handler, bulk_delete_handler,
    get_upload_status_handler, get_images_api_handler,
    update_tags_handler, toggle_like_handler, check_likes_handler,
    get_all_tags_handler, get_related_images_handler,
    get_search_suggestions_handler, dashboard_page_handler, get_dashboard_overview_handler,
    dashboard_activity_stream_handler, crawler_telemetry_middleware, get_disk_stats_handler,
    robots_txt_handler, sitemap_xml_handler, rss_xml_handler, tag_rss_xml_handler,
    get_comments_handler, add_comment_handler, delete_comment_handler,
    tag_page_handler
};
use crate::handlers::seo::{indexnow_key_handler, indexnow_route_path};
use crate::handlers::view::{
    image_page_handler, legacy_image_page_handler, media_file_handler, thumb_file_handler,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenv().ok();
    auth_init();

    fs::create_dir_all("uploads/thumbs").ok();
    fs::create_dir_all("run/ops-status").ok();

    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(50)
        .connect(&db_url)
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    let state = AppState::new(pool);

    spawn_session_cleaner(state.clone());

    let upload_routes = Router::new()
        .route("/api/upload", post(upload_handler))
        .layer(DefaultBodyLimit::max(536870912));

    let mut app = Router::new()
        .route("/", get(index_handler))
        .route("/image/:id/:slug", get(image_page_handler))
        .route("/image/:id", get(legacy_image_page_handler))
        .route("/tag/:slug/rss.xml", get(tag_rss_xml_handler))
        .route("/tag/:slug", get(tag_page_handler))
        .route("/media/:id/:slug", get(media_file_handler))
        .route("/thumbs/:id/:slug", get(thumb_file_handler))
        .route("/dashboard", get(dashboard_page_handler))
        .route("/api/admin/dashboard/overview", get(get_dashboard_overview_handler))
        .route("/api/admin/dashboard/activity-stream", get(dashboard_activity_stream_handler))
        .route("/api/admin/stats/disk", get(get_disk_stats_handler))
        .route("/api/images", get(get_images_api_handler))
        .route("/api/tags", get(get_all_tags_handler))
        .route("/api/search/suggestions", get(get_search_suggestions_handler))
        .route("/api/image/:id/tags", post(update_tags_handler))
        .route("/api/image/:id/like", post(toggle_like_handler))
        .route("/api/likes/check", post(check_likes_handler))
        .route("/api/image/:id/related", get(get_related_images_handler))
        .route("/api/image/:id/comments", get(get_comments_handler).post(add_comment_handler))
        .route("/api/comment/:id", delete(delete_comment_handler))
        .route("/api/login", post(login_handler))
        .route("/api/logout", post(logout_handler))
        .route("/api/upload/status/:id", get(get_upload_status_handler))
        .route("/api/delete/bulk", post(bulk_delete_handler))
        .route("/api/delete/:filename", delete(delete_handler))
        .route("/robots.txt", get(robots_txt_handler))
        .route("/sitemap.xml", get(sitemap_xml_handler))
        .route("/rss.xml", get(rss_xml_handler));

    if let Some(indexnow_path) = indexnow_route_path() {
        app = app.route(&indexnow_path, get(indexnow_key_handler));
    }

    let app = app
        .merge(upload_routes)
        .nest_service("/uploads", ServeDir::new("uploads"))
        .nest_service("/public", ServeDir::new("public"))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crawler_telemetry_middleware,
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_XSS_PROTECTION,
            HeaderValue::from_static("1; mode=block"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
        ))
        .layer(DefaultBodyLimit::max(2097152))
        .with_state(state);

    let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let addr: SocketAddr = format!("{}:{}", bind_addr, port)
        .parse()
        .expect("Invalid BIND_ADDR or PORT");
    println!("-> Backend <PUBLIC_DOMAIN> listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}
