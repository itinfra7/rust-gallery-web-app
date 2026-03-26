use sqlx::{Postgres, QueryBuilder};
use super::common::ImageQuery;

pub fn build_search_query<'a>(
    mut builder: QueryBuilder<'a, Postgres>,
    params: &'a ImageQuery,
    client_ip: &'a str
) -> QueryBuilder<'a, Postgres> {
    if params.fingerprint.is_some() {
        builder.push(" LEFT JOIN likes l ON i.id = l.image_id AND l.user_id = (SELECT id FROM users WHERE ip_address = ");
        builder.push_bind(client_ip);
        builder.push(" AND fingerprint = ");
        builder.push_bind(params.fingerprint.as_ref().unwrap());
        builder.push(")");
    } else {
        builder.push(" LEFT JOIN likes l ON FALSE ");
    }

    builder.push(" WHERE 1=1 ");

    if let Some(tag) = &params.tag {
        builder.push(" AND EXISTS (SELECT 1 FROM UNNEST(i.tags) t WHERE t ILIKE ");
        builder.push_bind(tag);
        builder.push(") ");
    }

    if let Some(search) = &params.search {
        let parts: Vec<&str> = search.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        
        for part in parts {
            if part.starts_with('-') {
                let term = part.trim_start_matches('-');
                if !term.is_empty() {
                    builder.push(" AND NOT EXISTS (SELECT 1 FROM UNNEST(i.tags) t WHERE t ILIKE ");
                    builder.push_bind(format!("%{}%", term));
                    builder.push(") ");
                }
            } else {
                builder.push(" AND EXISTS (SELECT 1 FROM UNNEST(i.tags) t WHERE t ILIKE ");
                builder.push_bind(format!("%{}%", part));
                builder.push(") ");
            }
        }
    }

    match params.sort.as_deref().unwrap_or("desc") {
        "asc" => builder.push(" ORDER BY i.upload_date ASC"),
        "popular" => builder.push(" ORDER BY i.like_count DESC, i.upload_date DESC"),
        "comments" => builder.push(" ORDER BY comment_count DESC, i.upload_date DESC"),
        _ => builder.push(" ORDER BY i.upload_date DESC"),
    };

    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(24).max(1);
    let offset = (page - 1) * limit;

    builder.push(" LIMIT ");
    builder.push_bind(limit);
    builder.push(" OFFSET ");
    builder.push_bind(offset);

    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Execute;

    fn build_sql(params: &ImageQuery) -> String {
        let builder = QueryBuilder::<Postgres>::new("SELECT * FROM images i");
        build_search_query(builder, params, "203.0.113.9")
            .build()
            .sql()
            .to_string()
    }

    #[test]
    fn query_without_fingerprint_keeps_likes_alias_valid() {
        let params = ImageQuery {
            sort: None,
            tag: None,
            page: Some(1),
            limit: Some(24),
            search: None,
            fingerprint: None,
        };

        let sql = build_sql(&params);
        assert!(sql.contains("LEFT JOIN likes l ON FALSE"));
        assert!(sql.contains("ORDER BY i.upload_date DESC"));
    }

    #[test]
    fn query_with_fingerprint_builds_like_join() {
        let params = ImageQuery {
            sort: Some("popular".to_string()),
            tag: Some("Skirt".to_string()),
            page: Some(1),
            limit: Some(12),
            search: None,
            fingerprint: Some("fp-123".to_string()),
        };

        let sql = build_sql(&params);
        assert!(sql.contains("LEFT JOIN likes l ON i.id = l.image_id"));
        assert!(sql.contains("fingerprint = "));
        assert!(sql.contains("ORDER BY i.like_count DESC, i.upload_date DESC"));
    }

    #[test]
    fn query_supports_negative_search_terms_and_comment_sort() {
        let params = ImageQuery {
            sort: Some("comments".to_string()),
            tag: None,
            page: Some(2),
            limit: Some(10),
            search: Some("cat, -dog".to_string()),
            fingerprint: None,
        };

        let sql = build_sql(&params);
        assert!(sql.contains("EXISTS (SELECT 1 FROM UNNEST(i.tags) t WHERE t ILIKE "));
        assert!(sql.contains("NOT EXISTS (SELECT 1 FROM UNNEST(i.tags) t WHERE t ILIKE "));
        assert!(sql.contains("ORDER BY comment_count DESC, i.upload_date DESC"));
        assert!(sql.contains(" LIMIT "));
        assert!(sql.contains(" OFFSET "));
    }
}
