use sqlx::{PgPool, Row};
use uuid::Uuid;
use chrono::Utc;

pub async fn get_or_create_user(
    pool: &PgPool,
    ip: &str,
    fingerprint: &str,
) -> Result<Uuid, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let user_id = match sqlx::query("SELECT id FROM users WHERE ip_address = $1 AND fingerprint = $2")
        .bind(ip)
        .bind(fingerprint)
        .fetch_optional(&mut *tx)
        .await? 
    {
        Some(row) => row.get("id"),
        None => {
            let new_id = Uuid::new_v4();
            sqlx::query("INSERT INTO users (id, ip_address, fingerprint) VALUES ($1, $2, $3)")
                .bind(new_id)
                .bind(ip)
                .bind(fingerprint)
                .execute(&mut *tx)
                .await?;
            new_id
        }
    };

    sqlx::query("UPDATE users SET last_activity_at = $1 WHERE id = $2")
        .bind(Utc::now())
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(user_id)
}
