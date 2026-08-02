use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub bot_token: String,
    pub sqlite_path: String,
    pub webhook_url: Option<String>,
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Settings, config::ConfigError> {
    config::Config::builder()
        .add_source(config::File::with_name(path.as_ref().to_str().unwrap()))
        .build()?
        .try_deserialize::<Settings>()
}
