use std::collections::HashMap;
use crate::sheets::{create_sheets_client};
use sqlx::sqlite::SqlitePool;
use std::net::SocketAddr;
use std::sync::Arc;
use teloxide::dispatching::Dispatcher;
use teloxide::{prelude::*, update_listeners::webhooks, utils::command::BotCommands};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use url::Url;
use crate::config::load_config;
use crate::handlers::cleanup_expired_callbacks;

mod db;
mod md;
mod sheets;
mod config;
mod handlers;
mod state;
mod utils;

const CONFIG_NAME: &str = "config.yaml";

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

    let settings = match load_config(config_path)
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

    let state = Arc::new(state::BotState {
        sheets,
        settings,
        sqlite_pool,
        tx,
        categories_hash: Mutex::new(HashMap::new()),
    });

    let state_for_channel = state.clone();

    tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ChannelCommand::Sync => {
                    if let Err(e) = handlers::handle_sync_message(state_for_channel.clone()).await {
                        log::error!("Failed to sync transaction: {}", e);
                    }
                }
            }
        }
    });


    let hash_items_ttl = std::time::Duration::from_secs(300);
    let refresh_duration = std::time::Duration::from_secs(60);

    let state_for_cleanup = state.clone();
    
    tokio::spawn(cleanup_expired_callbacks(state_for_cleanup, hash_items_ttl, refresh_duration));

    log::info!("Budgetbeast initialized");

    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(handlers::answer))
        .branch(Update::filter_callback_query().endpoint(handlers::callback_handler));

    let mut dispatcher = Dispatcher::builder(bot.clone(), handler)
        .dependencies(dptree::deps![state.clone()])
        .enable_ctrlc_handler()
        .build();

    match &state.settings.webhook_url {
        Some(url) => {
            let addr: SocketAddr = ([0, 0, 0, 0], 3333).into();
            let webhook_url: Url = url.parse().expect("Invalid url");
            let listener = webhooks::axum(bot, webhooks::Options::new(addr, webhook_url.clone()))
                .await
                .expect("Couldn't setup webhook");
            dispatcher
                .dispatch_with_listener(
                    listener,
                    LoggingErrorHandler::with_custom_text("An error from the update listener"),
                )
                .await
        }
        None => dispatcher.dispatch().await,
    }
}



