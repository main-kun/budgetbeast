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

pub struct Record {
    pub id: i64,
    pub date_created: String,
    pub amount: i64,
    pub category: String,
    pub username: String,
    pub synced_at: Option<String>,
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

pub async fn get_unsynced(pool: &SqlitePool) -> Result<Vec<Record>, sqlx::Error> {
    sqlx::query_as!(
        Record,
        r#"
        SELECT
            id,
            date_created,
            category,
            amount,
            username,
            synced_at,
            note
        FROM transactions WHERE synced_at IS NULL
        "#
    )
    .fetch_all(pool)
    .await
}

pub async fn update_synced_at(
    pool: &SqlitePool,
    time: DateTime<Utc>,
    ids: Vec<i64>,
) -> Result<(), sqlx::Error> {
    if ids.is_empty() {
        return Ok(());
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let query = format!(
        "UPDATE transactions SET synced_at = ? WHERE id IN ({}) AND synced_at IS NULL",
        placeholders
    );

    let mut query_with_args = sqlx::query(&query).bind(time.to_string());

    for id in ids {
        query_with_args = query_with_args.bind(id);
    }

    query_with_args.execute(pool).await?;

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
