use chrono::{DateTime, Utc};
use serde_json::json;
use teloxide::Bot;
use teloxide::{prelude::*};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Me};
use teloxide::utils::command::BotCommands;
use tokio_retry::Retry;
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use uuid::Uuid;
use crate::{state, utils, db,md, sheets, ChannelCommand, BotCommand};

async fn push_to_sheets(bot_state: state::SharedBotState) -> anyhow::Result<()> {
    let unsynced_rows = db::get_unsynced(&bot_state.sqlite_pool).await?;

    if unsynced_rows.is_empty() {
        log::debug!("No unsynced rows to push");
        return Ok(());
    }

    let ids: Vec<i64> = unsynced_rows.iter().map(|r| r.id).collect();
    let new_rows = unsynced_rows
        .iter()
        .filter_map(|row| match row.date_created.parse::<DateTime<Utc>>() {
            Ok(parsed_date) => {
                let formatted_date = parsed_date.format("%Y-%m-%d %H:%M:%S").to_string();
                Some(vec![
                    json!(formatted_date),
                    json!(row.category),
                    json!(utils::cents_to_full(row.amount)),
                    json!(row.note.clone().unwrap_or_default()),
                    json!(row.username),
                ])
            }
            Err(_) => {
                log::error!("Failed to parse date {}", row.date_created);
                None
            }
        })
        .collect::<Vec<Vec<serde_json::Value>>>();
    sheets::append_row(
        &bot_state.sheets,
        &bot_state.settings.spreadsheet.id,
        &bot_state.settings.spreadsheet.sheet_name,
        new_rows,
    )
        .await?;

    let now = Utc::now();
    db::update_synced_at(&bot_state.sqlite_pool, now, ids).await?;
    Ok(())
}


pub async fn handle_sync_message(bot_state: state::SharedBotState) -> anyhow::Result<()> {
    Retry::spawn(
        ExponentialBackoff::from_millis(100).map(jitter).take(5),
        || async { push_to_sheets(bot_state.clone()).await },
    )
        .await?;

    Ok(())
}

pub async fn cleanup_expired_callbacks(bot_state: state::SharedBotState, ttl: std::time::Duration, refresh: std::time::Duration) {
    loop {
        tokio::time::sleep(refresh).await;
        let mut map = bot_state.categories_hash.lock().await;
        let now = std::time::Instant::now();
        map.retain(|_, callback| now.duration_since(callback.created_at) < ttl);
        log::debug!("Cleaned up expired category callback")
    }
}


async fn add_command(bot: Bot, msg: Message, args: String, bot_state: state::SharedBotState) -> anyhow::Result<()> {
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
        send_category_menu(&bot, &msg, amount, note, bot_state).await?
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
    bot_state: state::SharedBotState
) -> anyhow::Result<()> {
    let categories = [
        "Groceries",
        "Delivery",
        "Cafe",
        "Eating out",
        "Transport",
        "Other",
    ];
    let mut map = bot_state.categories_hash.lock().await;
    let note_str = note.clone().unwrap_or_default();
    let menu_id = Uuid::new_v4();
    let category_tuples: Vec<(String, state::CategoryCallback)> = categories.iter().map(|&category| {
        (
            Uuid::new_v4().to_string(),
            state::CategoryCallback {
                menu_id,
                amount,
                category: category.to_string(),
                note: note_str.clone(),
                created_at: std::time::Instant::now(),
            }
        )
    }).collect();
    for (key, callback) in &category_tuples {
        map.insert(key.clone(), callback.clone());
    }
    let keyboard = category_tuples
        .chunks(2)
        .map(|chunk| {
            chunk
                .iter()
                .map(|(key, callback)| {  // Destructure the tuple here
                    InlineKeyboardButton::callback(
                        callback.category.clone(),
                        key.clone()
                    )
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


async fn edit_bot_message(bot: &Bot, q: &CallbackQuery, text: String) -> anyhow::Result<()> {
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


pub async fn callback_handler(bot: Bot, q: CallbackQuery, bot_state: state::SharedBotState) -> anyhow::Result<()> {
    if let Some(ref callback_data) = q.data {
        bot.answer_callback_query(&q.id).await?;
        log::debug!("Query data: {}", callback_data);


        let mut map = bot_state.categories_hash.lock().await;

        let category_data = match map.remove(callback_data) {
            Some(data) => data,
            None => {
                edit_bot_message(&bot, &q, "⛔ Invalid or expired selection".into()).await?;
                return Ok(())
            }
        };

        map.retain(|_, entry| entry.menu_id != category_data.menu_id);

        let amount_cents = (category_data.amount * 100.0).round() as i64;
        let category = category_data.category;
        let note =category_data.note;
        let username = q.from.username.clone().unwrap_or("unknown".into());

        match db::add_transaction(
            &bot_state.sqlite_pool,
            db::Transaction {
                date: Utc::now(),
                amount: amount_cents,
                category: category.clone(),
                username: username.clone(),
                note: Option::from(note.clone()),
            },
        )
            .await
        {
            Ok(_) => {
                let success_text = format!(
                    "✅ *Added transaction:*\n*Category:* {}\n*Amount:* {}\n",
                    category,
                    md::escape_markdown(utils::cents_to_full(amount_cents).to_string())
                );
                edit_bot_message(&bot, &q, success_text).await?;

                if let Err(e) = bot_state.tx.send(ChannelCommand::Sync).await {
                    log::error!("Failed to send sync command: {}", e);
                }
            }
            Err(e) => {
                log::error!("Failed to add transaction to db: {}", e);
                edit_bot_message(&bot, &q, String::from("⛔ Could not save the transaction"))
                    .await?;
                return Ok(());
            }
        };

        log::info!(
            "Transaction saved. Amount: {}; Category: {}; From: {}; Note: {:?}",
            utils::cents_to_full(amount_cents).to_string(),
            category,
            username,
            note
        );
    }

    Ok(())
}

pub async fn answer(bot: Bot, msg: Message, me: Me, bot_state: state::SharedBotState) -> anyhow::Result<()> {
    if let Some(text) = msg.text() {
        match BotCommands::parse(text, me.username()) {
            Ok(BotCommand::Add(command_value)) => {
                add_command(bot, msg.clone(), command_value, bot_state).await?;
            }
            Ok(BotCommand::Week) => {
                weekly_summary(bot,msg.clone(), bot_state).await?;
            }
            Err(_) => {
                let tokens: Vec<&str> = text.split_whitespace().collect();
                match tokens[0].parse::<f64>() {
                    Ok(_) => {
                        add_command(bot.clone(), msg.clone(), text.to_string(), bot_state).await?;
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

async fn weekly_summary(bot: Bot, msg: Message, bot_state: state::SharedBotState) -> anyhow::Result<()> {
    match db::get_weekly_summary(&bot_state.sqlite_pool).await {
        Ok(value) => {
            let response = format!("{} RSD", utils::cents_to_full(value));
            bot.send_message(msg.chat.id, response).await?;
        }
        Err(err) => {
            log::error!("Failed to retrieve weekly summary: {}", err);  
            bot.send_message(msg.chat.id, "Failed to retrieve weekly summary.").await?;
        }
    }
    Ok(())
}
