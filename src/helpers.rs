use std::num::ParseIntError;
use std::sync::Arc;

use serenity::all::{ChannelId, CreateActionRow, CreateButton, GuildId};
use serenity::async_trait;
use serenity::{all::Message, client::Context, Result as SerenityResult};
use songbird::tracks::TrackHandle;
use songbird::{Songbird, SongbirdKey};

use crate::audio;
use crate::audio::TrackHandleHelper;
use crate::commands::{PoiseContext, PoiseError, PoiseResult};
use crate::common::LogResult;
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

pub async fn is_bot_alone_in_voice_channel(
    ctx: &Context,
    guild_id: GuildId,
) -> Result<bool, PoiseError> {
    if let Some(bot_voice_channel_id) = get_bot_voice_channel_id(ctx, guild_id).await {
        if let Some(guild) = ctx.cache.guild(guild_id) {
            if let Some(channel) = guild.channels.get(&bot_voice_channel_id) {
                let members = channel.members(&ctx)?;
                return Ok(members.len() == 1 && members[0].user.id == ctx.cache.current_user().id);
            }
        }
    }

    Ok(false)
}

pub async fn get_bot_voice_channel_id(ctx: &Context, guild_id: GuildId) -> Option<ChannelId> {
    let user = ctx.cache.current_user();
    let bot_id = user.id;

    // Get the guild from the cache
    let guild = ctx.cache.guild(guild_id)?;

    // Get the voice states for the guild
    let voice_state = guild.voice_states.get(&bot_id)?;

    voice_state.channel_id
}

#[derive(Debug)]
pub enum ButtonCustomId {
    PlayAudio(i64),
    Unknown(String),
}

impl TryFrom<String> for ButtonCustomId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let parts: Vec<_> = value.split("::").collect();
        match parts[0] {
            "play" => {
                let id: i64 = parts[1]
                    .parse()
                    .map_err(|e: ParseIntError| e.to_string())
                    .log_err_op(|e| format!("Parse error on button custom id '{value}' - {e}"))?;
                Ok(ButtonCustomId::PlayAudio(id))
            }
            _ => Ok(ButtonCustomId::Unknown(value)),
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
    /// Begins play audio track and returns handle to track
    async fn play_audio(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError>;

    /// Plays audio track all the way to the end, then returns audio track
    async fn play_audio_to_end(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError>;

    async fn leave_voice_channel(&self, guild_id: GuildId) -> PoiseResult;
}

#[async_trait]
impl SongbirdHelper for Songbird {
    async fn leave_voice_channel(&self, guild_id: GuildId) -> PoiseResult {
        log::info!("Songbird leaving voice channel for guild_id: {guild_id}");

        match self.get(guild_id) {
            Some(_handler) => {
                self.leave(guild_id).await.log_err()?;
            }
            None => {
                log::error!("Songbird manager does not have a handler for guild_id: {guild_id}")
            }
        }

        Ok(())
    }

    async fn play_audio(
        &self,
        guild_id: GuildId,
        _channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError> {
        log::debug!("Starting to play_audio_track - {audio_track:?}");

        let audio_input = songbird::input::File::new(audio_track.as_path_buf());

        match self.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;

                let track_handle = handler.play_input(audio_input.into());
                log::info!("Playing track {audio_track:?}");
                Ok(track_handle)
            }
            None => Err(AudioError::NotInVoiceChannel),
        }
    }

    async fn play_audio_to_end(
        &self,
        guild_id: GuildId,
        _channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError> {
        log::debug!("Starting to play_audio_track - {audio_track:?}");

        let audio_input = songbird::input::File::new(audio_track.as_path_buf());

        match self.get(guild_id) {
            Some(handler_lock) => {
                let mut handler = handler_lock.lock().await;

                let track_handle = handler.play_input(audio_input.into());
                log::info!("Playing track {audio_track:?}");

                track_handle.wait_for_end().await;
                Ok(track_handle)
            }
            None => Err(AudioError::NotInVoiceChannel),
        }
    }
}

#[async_trait]
pub trait PoiseContextHelper<'a> {
    async fn songbird(&self) -> Arc<songbird::Songbird>;
}

#[async_trait]
impl<'a> PoiseContextHelper<'a> for PoiseContext<'a> {
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
            CreateButton::new(ButtonCustomId::PlayAudio(track.id))
                .label(track.name.to_button_label())
        })
        .collect();

    CreateActionRow::Buttons(buttons)
}

pub async fn autocomplete_audio_track_name<'a>(
    ctx: PoiseContext<'_>,
    partial: &'a str,
) -> impl futures::stream::Stream<Item = String> + 'a {
    let table = ctx.data().audio_table();
    let track_names = table.fts_autocomplete_track_names(partial, Some(5));
    futures::stream::iter(track_names)
}

pub async fn autocomplete_opt_audio_track_name<'a>(
    ctx: PoiseContext<'_>,
    partial: &'a str,
) -> impl futures::stream::Stream<Item = String> + 'a {
    let table = ctx.data().audio_table();
    let mut track_names = table.fts_autocomplete_track_names(partial, Some(5));
    track_names.insert(0, "NONE".into());

    futures::stream::iter(track_names)
}

pub fn uuid_v4_str() -> String {
    // Create uuid audio file in /tmp directory
    let uuid = uuid::Uuid::new_v4();
    let mut encode_buf = uuid::Uuid::encode_buffer();
    uuid.hyphenated().encode_lower(&mut encode_buf).to_string()
}

pub fn title_case(s: impl AsRef<str>) -> String {
    s.as_ref()
        .split_whitespace()
        .into_iter()
        .map(|s| {
            let mut it = s.chars();
            match it.next() {
                Some(c) => c.to_uppercase().to_string() + it.collect::<String>().as_str(),
                None => s.to_owned(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_case_test() {
        assert_eq!("This Is A Title", title_case("this is a title"));
        assert_eq!("This Is_a-title", title_case("this is_a-title"));
        assert_eq!("This Is A Title", title_case("this is\ta\t\ttitle"));
    }
}
