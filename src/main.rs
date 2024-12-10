use teloxide::{prelude::*, utils::command::BotCommands};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().expect("Could not load .env");
    pretty_env_logger::init();
    log::info!("Starting budgetbeast");
    
    let bot = Bot::from_env();
    
    Command::repl(bot, answer).await;
}

#[derive(BotCommands, Clone)]
#[command(rename_rule="lowercase", description="Supported commands:")]
enum Command {
    #[command(description = "Add transaction")]
    Add,
}

async fn answer(bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    match cmd {
        Command::Add => add_command(bot, msg).await?,
    };
    Ok(())
}

async fn add_command(bot: Bot, msg: Message) -> ResponseResult<()> {
    let username = msg
        .from
        .and_then(|user| user.username)
        .unwrap_or("unknown".to_string());
    
    log::info!("Received :add command call from user {}", username);
    bot.send_message(msg.chat.id, username).await?;
    Ok(())
}