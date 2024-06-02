use std::path;
use std::sync::{Arc, Mutex};

use r2d2_sqlite::rusqlite::Connection as DbConnection;
use reqwest::Client as HttpClient;
use serenity::all::{ChannelId, CreateActionRow, CreateButton, GuildId};
use serenity::async_trait;
use serenity::{all::Message, client::Context, prelude::TypeMapKey, Result as SerenityResult};
use songbird::tracks::TrackHandle;
use songbird::{Songbird, SongbirdKey};

use crate::audio;
use crate::commands::{GenericError, PoiseContext, PoiseError, PoiseResult};
use crate::config::Config;
use crate::db::AudioTableRow;
use crate::errors::AudioError;
use crate::vars;

pub async fn songbird_get(ctx: &Context) -> Arc<songbird::Songbird> {
    songbird::get(ctx)
        .await
        .expect("Songbird voice client placed in at initialization")
        .clone()
}

pub async fn poise_songbird_get(ctx: &PoiseContext<'_>) -> Arc<songbird::Songbird> {
    let data = ctx.serenity_context().data.read().await;
    data.get::<SongbirdKey>()
        .expect("Songbird voice client placed in at initialization")
        .clone()
}

/// check if message successfully sent, or log to error
pub fn check_msg(result: SerenityResult<Message>) {
    if let Err(err) = result {
        log::error!("Error sending message: {:?}", err);
    }
}

pub fn poise_check_msg(result: Result<poise::ReplyHandle, serenity::Error>) {
    if let Err(err) = result {
        log::error!("Error sending message: {:?}", err);
    }
}

#[derive(Debug)]
pub enum ButtonCustomId {
    PlayAudio(String),
    Unknown(String),
}

impl From<String> for ButtonCustomId {
    fn from(value: String) -> Self {
        let parts: Vec<_> = value.split("::").collect();
        match parts[0] {
            "play" => ButtonCustomId::PlayAudio(parts[1].into()),
            _ => ButtonCustomId::Unknown(value),
        }
    }
}

impl From<ButtonCustomId> for String {
    fn from(value: ButtonCustomId) -> Self {
        match value {
            ButtonCustomId::PlayAudio(val) => format!("play::{val}"),
            ButtonCustomId::Unknown(val) => format!("{val}"),
        }
    }
}

pub trait ButtonLabel {
    fn to_button_label(&self) -> String;
}

impl ButtonLabel for String {
    fn to_button_label(&self) -> String {
        truncate_button_label(&self)
    }
}

impl ButtonLabel for &str {
    fn to_button_label(&self) -> String {
        truncate_button_label(&self)
    }
}

pub fn truncate_button_label(label: impl AsRef<str>) -> String {
    let label = label.as_ref();
    if label.len() > vars::BTN_LABEL_MAX_LEN {
        format!("{}...", label[0..(vars::BTN_LABEL_MAX_LEN - 3)].to_string())
    } else {
        label.to_string()
    }
}

/// Get voice channel the author of command is currently in.
/// Returns tuple (guild_id, channel_id)
pub fn get_author_voice_channel(ctx: &PoiseContext) -> Result<(GuildId, ChannelId), PoiseError> {
    match ctx.guild() {
        Some(guild) => {
            let channel_id = guild
                .voice_states
                .get(&ctx.author().id)
                .and_then(|voice_state| voice_state.channel_id);

            match channel_id {
                Some(channel_id) => Ok((guild.id, channel_id)),
                None => Err(
                    "Unable to get author voice channel. Missing voice states channel id.".into(),
                ),
            }
        }
        None => Err("Unable to get author voice channel. Missing ctx.guild()".into()),
    }
}

#[async_trait]
pub trait SongbirdHelper {
    async fn play_audio(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: path::PathBuf,
    ) -> Result<TrackHandle, AudioError>;
}

#[async_trait]
impl SongbirdHelper for Songbird {
    async fn play_audio(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: path::PathBuf,
    ) -> Result<TrackHandle, AudioError> {
        log::debug!("Starting to play_audio_track - {audio_track:?}");

        let audio_input = songbird::input::File::new(audio_track.clone());

        match self.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;

                let track_handle = handler.play_input(audio_input.into());
                log::info!("Playing track {audio_track:?}");
                Ok(track_handle)
            }
            None => Err(AudioError::NotInVoiceChannel { guild_id: guild_id }),
        }
    }
}

#[async_trait]
pub trait PoiseContextHelper<'a> {
    fn config(&self) -> &Config;

    fn find_audio_track(&self, name: &str)
        -> Option<songbird::input::File<impl AsRef<path::Path>>>;

    async fn songbird(&self) -> Arc<songbird::Songbird>;
}

#[async_trait]
impl<'a> PoiseContextHelper<'a> for PoiseContext<'a> {
    fn config(&self) -> &Config {
        &self.data().config
    }

    fn find_audio_track(
        &self,
        name: &str,
    ) -> Option<songbird::input::File<impl AsRef<path::Path>>> {
        log::info!("Finding audio track by name - {name}");

        let audio_dir = self.config().audio_dir.clone();
        let audio_file_path = audio_dir.join(format!("{name}.mp3"));

        if audio_file_path.exists() {
            log::info!("Found audio track: {audio_file_path:?}");
            Some(songbird::input::File::new(audio_file_path))
        } else {
            log::error!("No audio track at: {audio_file_path:?}");
            None
        }
    }

    async fn songbird(&self) -> Arc<songbird::Songbird> {
        let data = self.serenity_context().data.read().await;
        data.get::<SongbirdKey>()
            .expect("Songbird voice client placed in at initialization")
            .clone()
    }
}

pub fn make_action_row(audio_rows: &[AudioTableRow]) -> CreateActionRow {
    let buttons: Vec<_> = audio_rows
        .iter()
        .map(|track| {
            CreateButton::new(ButtonCustomId::PlayAudio(track.id.to_string()))
                .label(track.name.to_button_label())
        })
        .collect();

    CreateActionRow::Buttons(buttons)
}
