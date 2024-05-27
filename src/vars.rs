pub const BTN_LABEL_MAX_LEN: usize = 80;
pub const BTN_CUSTOM_ID_MAX_LEN: usize = 80;
pub const CUSTOM_ID_SEP: &str = "::";
pub enum CustomIdCommand {
    Play,
}

pub mod env {
    use std::{any::type_name, fmt::Debug, str::FromStr};

    pub const DISCORD_BOT_APPLICATION_ID: &str = "DISCORD_BOT_APPLICATION_ID";
    pub const DISCORD_BOT_TOKEN: &str = "DISCORD_BOT_TOKEN";
    pub const DISCORD_BOT_AUDIO_DIR: &str = "DISCORD_BOT_AUDIO_DIR";
    pub const DISCORD_BOT_DOTENV_FILE: &str = "DISCORD_BOT_DOTENV_FILE";
    pub const DISCORD_BOT_COMMAND_PREFIX: &str = "DISCORD_BOT_COMMAND_PREFIX";
    pub const DISCORD_BOT_JOIN_AUDIO: &str = "DISCORD_BOT_JOIN_AUDIO";
    pub const DISCORD_BOT_LEAVE_AUDIO: &str = "DISCORD_BOT_LEAVE_AUDIO";

    /// Simple wrapper to get env vars and use default values on some env variables
    pub fn get<'a, T>(name: impl Into<&'a str>) -> T
    where
        T: FromStr,
        <T as FromStr>::Err: std::fmt::Debug,
    {
        let name = name.into();

        let expect_msg = format!("Missing {name} environment variable value");
        let expect_msg = expect_msg.as_str();

        let val = match name {
            DISCORD_BOT_AUDIO_DIR => std::env::var(name).unwrap_or("./audio".into()),
            DISCORD_BOT_COMMAND_PREFIX => std::env::var(name).unwrap_or("sb:".into()),
            DISCORD_BOT_DOTENV_FILE => std::env::var(name).unwrap_or(".env".into()),
            DISCORD_BOT_JOIN_AUDIO => std::env::var(name).unwrap_or("".into()), //default disabled
            DISCORD_BOT_LEAVE_AUDIO => std::env::var(name).unwrap_or("".into()), //default disabled
            _ => std::env::var(name).expect(expect_msg),
        };

        val.parse::<T>().expect(
            format!(
                "Failed to parse env var {name} to type {}",
                type_name::<T>()
            )
            .as_str(),
        )
    }

    pub fn try_get<'a, T>(name: impl Into<&'a str>) -> Option<T>
    where
        T: FromStr,
        <T as FromStr>::Err: std::fmt::Debug,
    {
        let name = name.into();

        let expect_msg = format!("Missing {name} environment variable value");
        let expect_msg = expect_msg.as_str();

        match std::env::var(name) {
            Ok(val) => {
                let value = val.parse::<T>().expect(
                    format!(
                        "Failed to parse env var {name} to type {}",
                        type_name::<T>()
                    )
                    .as_str(),
                );

                Some(value)
            }
            Err(_) => None,
        }
    }
}
