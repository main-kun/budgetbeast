use crate::config;
use sqlx::sqlite::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct CategoryCallback {
    pub category: String,
    pub amount: f64,
    pub note: String,
    pub menu_id: Uuid,
    pub created_at: std::time::Instant,
}

pub struct BotState {
    pub settings: config::Settings,
    pub sqlite_pool: SqlitePool,
    pub categories_hash: Mutex<HashMap<String, CategoryCallback>>,
}

pub type SharedBotState = Arc<BotState>;
