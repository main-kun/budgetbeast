use std::sync::Arc;
use google_sheets4::Sheets;
use teloxide::{prelude::*, utils::command::BotCommands};
use serde_json::json;
use serde::Deserialize;
use crate::sheets::{append_row, create_sheets_client, SheetsClient};

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

    let state_clone = Arc::clone(&state);
    Command::repl(
        bot, 
        move |bot, msg, cmd| answer(bot, msg, cmd, Arc::clone(&state_clone)
    )
    ).await;
}

#[derive(BotCommands, Clone)]
#[command(rename_rule="lowercase", description="Supported commands:")]
enum Command {
    #[command(description = "Add transaction")]
    Add,
}

async fn answer(bot: Bot, msg: Message, cmd: Command, bot_state: Arc<BotState>) -> ResponseResult<()> {
    match cmd {
        Command::Add => add_command(bot, msg, &bot_state).await?,
    };
    Ok(())
}

async fn add_command(bot: Bot, msg: Message, bot_state: &BotState) -> ResponseResult<()> {
    let username = msg
        .from
        .and_then(|user| user.username)
        .unwrap_or("unknown".to_string());

    log::info!("Received :add command call from user {}", username);
    append_row(
        &bot_state.sheets,
        &bot_state.settings.spreadsheet.id,
        &bot_state.settings.spreadsheet.sheet_name,
        vec![
            vec![
                json!("2024-12-13"),
                json!("SOFTWARE"),
                json!("500")
            ]
        ]
    ).await.expect("Could not write the row");
    bot.send_message(msg.chat.id, username).await?;
    Ok(())
}