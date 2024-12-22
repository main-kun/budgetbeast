use anyhow::Result;
use sqlx::{Acquire, SqlitePool};

pub struct Transaction {
    pub date: String,
    pub amount: i32,
    pub category: String,
    pub username: String,
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
