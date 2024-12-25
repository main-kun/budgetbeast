use crate::db::{add_transaction, get_unsynced, update_synced_at, Transaction};
use crate::md::escape_markdown;
use crate::sheets::{append_row, create_sheets_client, SheetsClient};
use anyhow::Result;
use chrono::Utc;
use google_sheets4::Sheets;
use serde::Deserialize;
use serde_json::json;
use sqlx::sqlite::SqlitePool;
use std::sync::Arc;
use teloxide::dispatching::Dispatcher;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Me};
use teloxide::{prelude::*, utils::command::BotCommands};
use tokio::sync::mpsc;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::Retry;

mod db;
mod md;
mod sheets;

const CONFIG_NAME: &str = "config.yaml";
#[derive(Debug, Deserialize)]
struct Settings {
    spreadsheet: SpreadsheetSettings,
    service_account_key: String,
    bot_token: String,
    sqlite_path: String,
}

#[derive(Debug, Deserialize)]
struct SpreadsheetSettings {
    id: String,
    sheet_name: String,
}

struct BotState {
    sheets: Sheets<SheetsClient>,
    settings: Settings,
    sqlite_pool: SqlitePool,
    tx: mpsc::Sender<ChannelCommand>,
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Supported commands:")]
enum BotCommand {
    #[command(description = "Add transaction")]
    Add(String),
}

enum ChannelCommand {
    Sync,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let exec_path = std::env::current_exe().expect("Could not get execution directory");
    let exec_dir = exec_path.parent().unwrap();
    let config_path = exec_dir.join(CONFIG_NAME);

    let (tx, mut rx) = mpsc::channel::<ChannelCommand>(32);

    let settings = match config::Config::builder()
        .add_source(config::File::with_name(config_path.to_str().unwrap()))
        .build()
        .and_then(|c| c.try_deserialize::<Settings>())
    {
        Ok(settings) => settings,
        Err(e) => {
            eprintln!("Failed to load bot settings: {}", e);
            std::process::exit(1)
        }
    };

    let bot = Bot::new(&settings.bot_token);

    let sheets = match create_sheets_client(&settings.service_account_key).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to create sheets client: {}", e);
            std::process::exit(1);
        }
    };

    let sqlite_pool = match SqlitePool::connect(&settings.sqlite_path).await {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("Failed to create sqlite client: {}", e);
            std::process::exit(1);
        }
    };

    let state = Arc::new(BotState {
        sheets,
        settings,
        sqlite_pool,
        tx,
    });

    let state_for_channel = state.clone();

    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ChannelCommand::Sync => {
                    if let Err(e) = handle_sync_message(state_for_channel.clone()).await {
                        log::error!("Failed to sync transaction: {}", e);
                    }
                }
            }
        }
    });

    log::info!("Budgetbeast initialized");

    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(answer))
        .branch(Update::filter_callback_query().endpoint(callback_handler));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state.clone()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

async fn push_to_sheets(bot_state: Arc<BotState>) -> Result<()> {
    let unsynced_rows = get_unsynced(&bot_state.sqlite_pool).await?;

    if unsynced_rows.is_empty() {
        log::debug!("No unsynced rows to push");
        return Ok(());
    }

    let ids: Vec<i64> = unsynced_rows.iter().map(|r| r.id).collect();
    let new_rows = unsynced_rows
        .iter()
        .map(move |row| {
            vec![
                json!(row.date_created),
                json!(row.category),
                json!(row.amount),
                json!(row.username),
                json!(row.note.clone().unwrap_or_default()),
            ]
        })
        .collect::<Vec<Vec<serde_json::Value>>>();
    append_row(
        &bot_state.sheets,
        &bot_state.settings.spreadsheet.id,
        &bot_state.settings.spreadsheet.sheet_name,
        new_rows,
    )
    .await?;

    let now = Utc::now();
    update_synced_at(&bot_state.sqlite_pool, now, ids).await?;
    Ok(())
}

async fn handle_sync_message(bot_state: Arc<BotState>) -> Result<()> {
    Retry::spawn(
        ExponentialBackoff::from_millis(100).map(jitter).take(5),
        || async { push_to_sheets(bot_state.clone()).await },
    )
    .await?;

    Ok(())
}

async fn answer(bot: Bot, msg: Message, me: Me) -> Result<()> {
    if let Some(text) = msg.text() {
        match BotCommands::parse(text, me.username()) {
            Ok(BotCommand::Add(amount)) => {
                add_command(bot, msg, amount).await?;
            }
            Err(_) => {
                let normalized = text.replace(",", ".");
                match normalized.parse::<f64>() {
                    Ok(amount) => {
                        add_command(bot.clone(), msg.clone(), amount.to_string()).await?;
                    }
                    Err(_) => {
                        bot.send_message(msg.chat.id, "Unknown command").await?;
                    }
                };
            }
        }
    }
    Ok(())
}

async fn add_command(bot: Bot, msg: Message, args: String) -> Result<()> {
    let username = msg
        .from
        .as_ref()
        .and_then(|user| user.username.clone())
        .unwrap_or("unknown".to_string());

    let tokens: Vec<&str> = args.split_whitespace().collect();
    if tokens.is_empty() {
        bot.send_message(msg.chat.id, "Invalid message").await?;
        return Ok(());
    }
    let amount_str = tokens[0];

    let note = if tokens.len() > 1 {
        Some(tokens[1..].join(" "))
    } else {
        None
    };
    log::info!("Received :add command call from user {}", username);
    if let Ok(amount) = amount_str.replace(",", ".").parse::<f64>() {
        send_category_menu(&bot, &msg, amount, note).await?
    } else {
        bot.send_message(msg.chat.id, "Invalid amount").await?;
    }
    Ok(())
}
async fn send_category_menu(
    bot: &Bot,
    msg: &Message,
    amount: f64,
    note: Option<String>,
) -> Result<()> {
    let categories = [
        "Groceries",
        "Delivery",
        "Cafe",
        "Eating out",
        "Transport",
        "Other",
    ];
    let keyboard = categories
        .chunks(2)
        .map(|chunk| {
            chunk
                .iter()
                .map(|category| {
                    let note_str = note.clone().unwrap_or_default();
                    let callback_data = format!("category:{}:{}:{}", category, amount, note_str);
                    InlineKeyboardButton::callback(category.to_string(), callback_data)
                })
                .collect::<Vec<InlineKeyboardButton>>()
        })
        .collect::<Vec<Vec<InlineKeyboardButton>>>();

    let markup = InlineKeyboardMarkup::new(keyboard);
    bot.send_message(msg.chat.id, "Choose a category")
        .reply_markup(markup)
        .await?;

    Ok(())
}

async fn callback_handler(bot: Bot, q: CallbackQuery, bot_state: Arc<BotState>) -> Result<()> {
    if let Some(ref callback_data) = q.data {
        bot.answer_callback_query(&q.id).await?;
        log::debug!("Query data: {}", callback_data);

        let parts: Vec<&str> = callback_data.split(":").collect();

        if parts.len() < 3 || parts[0] != "category" {
            edit_bot_message(&bot, &q, String::from("⛔ Could not parse the response")).await?;
            return Ok(());
        }

        let category = parts[1].to_string();

        let amount = match parts[2].parse::<f32>() {
            Ok(num) => (num * 100.0).round() as i32,
            Err(_) => {
                edit_bot_message(&bot, &q, String::from("⛔ Could not parse amount")).await?;
                return Ok(());
            }
        };

        let note = if parts.len() > 3 {
            Some(parts[3..].join(":"))
        } else {
            None
        };

        let username = q.from.username.clone().unwrap_or("unknown".into());
        let utc = Utc::now();

        match add_transaction(
            &bot_state.sqlite_pool,
            Transaction {
                date: utc.to_string(),
                amount,
                category: category.clone(),
                username: username.clone(),
                note: note.clone(),
            },
        )
        .await
        {
            Ok(_) => {
                let success_text = format!(
                    "✅ *Added transaction:*\n*Category:* {}\n*Amount:* {}\n",
                    category,
                    escape_markdown(amount.to_string())
                );
                edit_bot_message(&bot, &q, success_text).await?;

                if let Err(e) = bot_state.tx.send(ChannelCommand::Sync).await {
                    log::error!("Failed to send sync command: {}", e);
                }
            }
            Err(_) => {
                edit_bot_message(&bot, &q, String::from("⛔ Could not save the transaction"))
                    .await?;
                return Ok(());
            }
        };

        log::info!(
            "Transaction saved. Amount: {}; Category: {}; From: {}; Note: {:?}",
            amount,
            category,
            username,
            note
        );
    }

    Ok(())
}

async fn edit_bot_message(bot: &Bot, q: &CallbackQuery, text: String) -> Result<()> {
    if let Some(message) = q.regular_message() {
        bot.edit_message_text(message.chat.id, message.id, text)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    } else if let Some(id) = q.inline_message_id.clone() {
        bot.edit_message_text_inline(id, text)
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
    }
    Ok(())
}
