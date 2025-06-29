use std::{env, path, str::FromStr};

use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub application_id: u64,
    pub token: String,
    #[serde(default = "default_audio_dir")]
    pub audio_dir: path::PathBuf,
    #[serde(default = "default_command_prefix")]
    pub command_prefix: String,
    #[serde(default = "default_sqlite_db_file")]
    pub sqlite_db_file: path::PathBuf,
    #[serde(
        default = "default_max_audio_file_duration",
        deserialize_with = "de_max_audio_file_duration"
    )]
    pub max_audio_file_duration: std::time::Duration,
    #[serde(default = "default_max_page_size")]
    pub max_page_size: u64,
    #[serde(default = "default_enable_ephemeral_controls")]
    pub enable_ephemeral_controls: bool,
}

impl Config {
    pub fn new() -> Self {
        let env_file = env::var("DISCORD_BOT_DOTENV_FILE");
        let env_file = env_file.unwrap_or("./.env".into());
        dotenv::from_filename(env_file).ok();

        let c = config::Config::builder()
            .add_source(config::Environment::with_prefix("discord_bot"))
            .build()
            .unwrap_or_else(|err| panic!("Missing/Incorrect environment variables - {err}"));

        let cfg: Config = c
            .try_deserialize()
            .unwrap_or_else(|err| panic!("Failed deserializing config - {err}"));

        cfg.validate();

        cfg
    }

    pub fn validate(&self) {
        let mut errs: Vec<String> = vec![];

        self.validate_audio_dir().map_err(|err| errs.push(err)).ok();

        if errs.len() > 0 {
            let err_msg: String = errs.iter().map(|err| format!("{err}\n")).collect();
            panic!("{}", err_msg);
        }
    }

    fn validate_audio_dir(&self) -> Result<(), String> {
        if !self.audio_dir.exists() {
            return Err(format!(
                "Audio directory does not exist - {}",
                self.audio_dir.to_str().unwrap_or("")
            ));
        }

        if !self.audio_dir.is_dir() {
            return Err(format!(
                "Audio directory path is not a directory - {}",
                self.audio_dir.to_str().unwrap_or("")
            ));
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            application_id: 0,
            token: "".into(),
            audio_dir: default_audio_dir(),
            command_prefix: default_command_prefix(),
            sqlite_db_file: default_sqlite_db_file(),
            max_audio_file_duration: default_max_audio_file_duration(),
            max_page_size: default_max_page_size(),
            enable_ephemeral_controls: default_enable_ephemeral_controls(),
        }
    }
}

fn default_enable_ephemeral_controls() -> bool {
    true
}

fn default_max_page_size() -> u64 {
    20
}

fn default_audio_dir() -> path::PathBuf {
    path::PathBuf::from_str("./audio").unwrap()
}

fn default_command_prefix() -> String {
    "sb:".into()
}

fn default_sqlite_db_file() -> path::PathBuf {
    path::PathBuf::from_str("./bot.db3").unwrap()
}

pub fn default_max_audio_file_duration() -> std::time::Duration {
    std::time::Duration::from_secs(7)
}

pub fn de_max_audio_file_duration<'de, D>(deserializer: D) -> Result<std::time::Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let value = u64::deserialize(deserializer)?;
    Ok(std::time::Duration::from_millis(value))
}
