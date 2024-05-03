use std::{any::type_name, env, fmt::Debug, str::FromStr};

pub const DISCORD_BOT_APPLICATION_ID: &str = "DISCORD_APPLICATION_ID";
pub const DISCORD_BOT_TOKEN: &str = "DISCORD_BOT_TOKEN";
pub const DISCORD_BOT_AUDIO_DIR: &str = "DISCORD_BOT_AUDIO_DIR";
pub const DISCORD_BOT_DOTENV_FILE: &str = "DISCORD_BOT_DOTENV_FILE";
pub const DISCORD_BOT_COMMAND_PREFIX: &str = "DISCORD_BOT_COMMAND_PREFIX";

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
        DISCORD_BOT_AUDIO_DIR => env::var(name).unwrap_or("./audio".into()),
        DISCORD_BOT_COMMAND_PREFIX => env::var(name).unwrap_or("sb:".into()),
        DISCORD_BOT_DOTENV_FILE => env::var(name).unwrap_or(".env".into()),
        _ => env::var(name).expect(expect_msg),
    };

    val.parse::<T>().expect(
        format!(
            "Failed to parse env var {name} to type {}",
            type_name::<T>()
        )
        .as_str(),
    )
}
