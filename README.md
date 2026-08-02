# BudgetBeast

A Telegram bot for personal budget tracking. Records and categorizes expenses using local SQLite storage.

## Features

- Track expenses via Telegram — send an amount or use `/add <amount> [note]`
- Categorize spending (Groceries, Delivery, Cafe, Eating out, Transport, Other)
- Weekly spending summaries with `/week`
- SQLite for local persistence
- Multi-user support via Telegram usernames

## Setup

1. Create a `config.yaml`:

```yaml
bot_token: "your-telegram-bot-token"
sqlite_path: "sqlite:///data.db"
# webhook_url: "https://example.com"  # optional, uses polling if omitted
```

2. Run with Docker Compose:

```sh
docker compose up
```

Or build and run directly:

```sh
cargo build --release
./target/release/budgetbeast
```
