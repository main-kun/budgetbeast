use std::sync::Arc;
use google_sheets4::Sheets;
use teloxide::{prelude::*, utils::command::BotCommands};
use teloxide::dispatching::Dispatcher;
use serde::Deserialize;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Me};
use crate::sheets::{append_row, create_sheets_client, SheetsClient};
use anyhow::Result;
use chrono::Utc;
use serde_json::json;

pub mod sheets;

const CONFIG_NAME: &str = "config.yaml";
#[derive(Debug, Deserialize)]
struct Settings {
    spreadsheet: SpreadsheetSettings,
    service_account_key: String,
    bot_token: String
}

#[derive(Debug, Deserialize)]
struct SpreadsheetSettings {
    id: String,
    sheet_name: String
}

struct BotState {
    sheets: Sheets<SheetsClient>,
    settings: Settings
}

#[derive(BotCommands, Clone)]
#[command(rename_rule="lowercase", description="Supported commands:")]
enum Command {
    #[command(description = "Add transaction")]
    Add(String),
}


#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let exec_path = std::env::current_exe().expect("Could not get execution directory");
    let exec_dir = exec_path.parent().unwrap();
    let config_path = exec_dir.join(CONFIG_NAME);
    
    let settings = match config::Config::builder()
        .add_source(config::File::with_name(config_path.to_str().unwrap()))
        .build()
        .and_then(|c| c.try_deserialize::<Settings>()) { 
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
    
    let state = Arc::new(BotState {
        sheets,
        settings
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


async fn answer(bot: Bot, msg: Message, me: Me) -> Result<()> {
    if let Some(text) = msg.text() {
        match BotCommands::parse(text, me.username()) {
            Ok(Command::Add(amount)) => {
                add_command(bot, msg, amount).await?;
            }
            Err(_) => {
                let normalized = text.replace(",", ".");
                match normalized.parse::<f64>() {
                    Ok(amount) => {
                        add_command(bot.clone(), msg.clone(), amount.to_string()).await?;
                    },
                    Err(_) => {
                        bot.send_message(msg.chat.id, "Unknown command").await?;
                    }
                };
            }
        }
    }
    Ok(())
}

async fn add_command(bot: Bot, msg: Message, amount: String) -> Result<()> {
    let username = msg
        .from
        .as_ref()
        .and_then(|user| user.username.clone())
        .unwrap_or("unknown".to_string());

    log::info!("Received :add command call from user {}", username);
    if let Ok(amount) = amount.parse::<f64>() {
        send_category_menu(&bot, &msg, amount)
            .await?
    } else {
        bot.send_message(msg.chat.id, "Invalid amount").await?;
    }
    Ok(())
}
async fn send_category_menu(bot: &Bot, msg: &Message, amount: f64) -> Result<()> {
    let categories = ["Groceries", "Delivery", "Cafe", "Eating out", "Transport", "Other"];
    let keyboard = categories
        .chunks(2)
        .map(|chunk| {
            chunk.iter().map(|category| {
               InlineKeyboardButton::callback(
                   category.to_string(),
                   format!("category:{}:{}", category, amount))
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
    if let Some(ref category) = q.data {
        bot.answer_callback_query(&q.id).await?;
        log::debug!("Query data: {}", category);

        let parts: Vec<&str> = category.split(":").collect();

        if parts.len() != 3 || parts[0] != "category" {
            edit_bot_message(
                &bot,
                &q,
                String::from("⛔ Could not parse the response")
            ).await?;
        }

        let category = parts[1].to_string();
        let amount = match parts[2].parse::<f64>()  {
            Ok(num) => num,
            Err(_) => {
                edit_bot_message(
                    &bot,
                    &q,
                    String::from("⛔ Could not parse amount")
                ).await?;
                return Ok(())
            }
        };
        
        let utc = Utc::now();

       match append_row(
            &bot_state.sheets,
            &bot_state.settings.spreadsheet.id,
            &bot_state.settings.spreadsheet.sheet_name,
            vec![
                vec![
                    json!(utc),
                    json!(category),
                    json!(amount)
                ]
            ]
        ).await {
           Ok(_) => {
               let success_text = format!(
                   "✅ *Added transaction:*\n*Category:* {}\n*Amount:* {}\n",
                   category,
                   amount.to_string().replace(".", r"\.")
               );
               edit_bot_message(&bot, &q, success_text).await?;
           }
           Err(_) => {
               edit_bot_message(
                   &bot,
                   &q,
                   String::from("⛔ Could not save the transaction")
               ).await?;
               return Ok(())
           }
       };

        log::info!(
            "Transaction saved. Amount: {}; Category: {}; From: {}",
            amount,
            category,
            q.from.username.unwrap_or(String::from("unknown"))
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
            .parse_mode(teloxide::types::ParseMode::MarkdownV2).await?;
    }
    Ok(())
}