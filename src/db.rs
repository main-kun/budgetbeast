use anyhow::Result;
use serde_json::error::Category;
use sqlx::{Acquire, SqlitePool};

pub struct Transaction {
    pub date: String,
    pub amount: i32,
    pub category: String,
    pub username: String,
}

pub struct Record {
    pub date_created: String,
    pub id: i64,
    pub amount: i64,
    pub category: String,
    pub username: String,
    pub synced_at: Option<String>,
}

pub async fn add_transaction(pool: &SqlitePool, transaction: Transaction) -> Result<()> {
    let mut conn = pool.acquire().await?;

    sqlx::query!(
        r#"
        INSERT INTO transactions (date_created, category, amount, username)
        VALUES ( ?1, ?2, ?3, ?4)
        "#,
        transaction.date,
        transaction.category,
        transaction.amount,
        transaction.username
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get_unsynced(pool: &SqlitePool) -> Result<Vec<Record>, sqlx::Error> {
    sqlx::query_as!(
        Record,
        r#"
        SELECT * FROM transactions WHERE synced_at IS NULL
        "#
    )
    .fetch_all(pool)
    .await
}
