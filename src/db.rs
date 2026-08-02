use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

pub struct Transaction {
    pub date: DateTime<Utc>,
    pub amount: i64,
    pub category: String,
    pub username: String,
    pub note: Option<String>,
}

pub async fn add_transaction(pool: &SqlitePool, transaction: Transaction) -> Result<()> {
    let mut conn = pool.acquire().await?;

    sqlx::query!(
        r#"
        INSERT INTO transactions (date_created, category, amount, username, note)
        VALUES ( ?1, ?2, ?3, ?4, ?5)
        "#,
        transaction.date,
        transaction.category,
        transaction.amount,
        transaction.username,
        transaction.note
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get_weekly_summary(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
        SELECT
            COALESCE(SUM(amount), 0) AS "sum: i64"
        FROM transactions
        WHERE date(date_created) >= date(
            'now',
            'start of day',
            '-' || ((strftime('%w', 'now') + 6) % 7) || ' days'
            )
        "#
    )
    .fetch_one(pool)
    .await
}
