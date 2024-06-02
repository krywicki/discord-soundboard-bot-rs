use std::{any::type_name, env, path, str::FromStr};

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub application_id: u64,
    pub token: String,
    #[serde(default = "default_audio_dir")]
    pub audio_dir: path::PathBuf,
    #[serde(default = "default_command_prefix")]
    pub command_prefix: String,
    pub join_audio: Option<String>,
    pub leave_audio: Option<String>,
    #[serde(default = "default_sqlite_db_file")]
    pub sqlite_db_file: path::PathBuf,
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

        self.validate_audio_dir().map_err(|err| errs.push(err));

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
            audio_dir: "".into(),
            command_prefix: "".into(),
            join_audio: None,
            leave_audio: None,
            sqlite_db_file: "".into(),
        }
    }
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
