# BudgetBeast

A Telegram bot for personal budget tracking. Records expenses, categorizes them, and syncs everything to Google Sheets.

## Features

- Track expenses via Telegram — send an amount or use `/add <amount> [note]`
- Categorize spending (Groceries, Delivery, Cafe, Eating out, Transport, Other)
- Weekly spending summaries with `/week`
- Auto-sync to Google Sheets
- SQLite for local persistence
- Multi-user support via Telegram usernames

## Setup

1. Create a `config.yaml`:

```yaml
bot_token: "your-telegram-bot-token"
service_account_key: "path/to/service-account.json"
sqlite_path: "sqlite:///data.db"
spreadsheet:
  id: "your-google-sheet-id"
  sheet_name: "Sheet1"
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
