use std::env;
use std::sync::Arc;
use google_sheets4::Sheets;
use teloxide::{prelude::*, utils::command::BotCommands};
use serde_json::json;
use crate::sheets::{append_row, create_sheets_client, SheetsClient};

pub mod sheets;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().expect("Could not load .env");
    pretty_env_logger::init();

    let client_path = env::var("SERVICE_ACCOUNT_KEY")
        .expect("env variable \"SERVICE_ACCOUNT_KEY\" was not found");
    log::info!("Starting budgetbeast");

    let sheets = match create_sheets_client(&client_path).await {
        Ok(client) => Arc::new(client),
        Err(e) => {
            eprintln!("Failed to create sheets client: {}", e);
            std::process::exit(1);
        }
    };
    let bot = Bot::from_env();
    
    let sheets_clone = Arc::clone(&sheets);
    Command::repl(
        bot, 
        move |bot, msg, cmd| answer(bot, msg, cmd, Arc::clone(&sheets_clone)
    )
    ).await;
}

#[derive(BotCommands, Clone)]
#[command(rename_rule="lowercase", description="Supported commands:")]
enum Command {
    #[command(description = "Add transaction")]
    Add,
}

async fn answer(bot: Bot, msg: Message, cmd: Command, sheets: Arc<Sheets<SheetsClient>>) -> ResponseResult<()> {
    match cmd {
        Command::Add => add_command(bot, msg, &sheets).await?,
    };
    Ok(())
}

async fn add_command(bot: Bot, msg: Message, sheets: &Sheets<SheetsClient>) -> ResponseResult<()> {
    let username = msg
        .from
        .and_then(|user| user.username)
        .unwrap_or("unknown".to_string());

    log::info!("Received :add command call from user {}", username);
    let spreadsheet_id = env::var("SPREADSHEET_ID")
        .expect("Expected SPREADSHEET_ID env var");
    append_row(
        sheets,
        &spreadsheet_id,
        "Sheet1",
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