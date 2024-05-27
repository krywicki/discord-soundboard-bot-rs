use std::sync::{Arc, Mutex};

use r2d2_sqlite::rusqlite::Connection as DbConnection;
use reqwest::Client as HttpClient;
use serenity::all::{ChannelId, GuildId};
use serenity::async_trait;
use serenity::{all::Message, client::Context, prelude::TypeMapKey, Result as SerenityResult};
use songbird::tracks::TrackHandle;
use songbird::{Songbird, SongbirdKey};

use crate::commands::{GenericError, PoiseContext, PoiseError, PoiseResult};
use crate::errors::AudioError;
use crate::{audio, vars};

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

pub fn truncate_button_label(label: &String) -> String {
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
        audio_track: &str,
    ) -> Result<TrackHandle, AudioError>;
}

#[async_trait]
impl SongbirdHelper for Songbird {
    async fn play_audio(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: &str,
    ) -> Result<TrackHandle, AudioError> {
        log::debug!("Starting to play_audio_track - {audio_track}");

        match self.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;

                match audio::find_audio_track(audio_track) {
                    Some(audio_track_input) => {
                        let track_handle = handler.play_input(audio_track_input.into());
                        log::info!("Playing track {}", audio_track);

                        return Ok(track_handle);
                    }
                    None => {
                        return Err(AudioError::AudioTrackNotFound {
                            track: "hello".into(), //audio_track.as_ref().to_string(),
                        }
                        .into());
                    }
                }
            }
            None => Err(AudioError::NotInVoiceChannel { guild_id: guild_id }),
        }
    }
}
