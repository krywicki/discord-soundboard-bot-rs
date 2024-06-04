use std::num::ParseIntError;
use std::path;
use std::sync::{Arc, Mutex};

use r2d2_sqlite::rusqlite::Connection as DbConnection;
use reqwest::Client as HttpClient;
use serenity::all::{ChannelId, CreateActionRow, CreateButton, GuildId};
use serenity::async_trait;
use serenity::{all::Message, client::Context, prelude::TypeMapKey, Result as SerenityResult};
use songbird::tracks::TrackHandle;
use songbird::{Songbird, SongbirdKey};

use crate::commands::{GenericError, PoiseContext, PoiseError, PoiseResult};
use crate::common::LogResult;
use crate::config::Config;
use crate::db::{AudioTable, AudioTableRow, FtsText};
use crate::errors::AudioError;
use crate::vars;
use crate::{audio, db};

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
    async fn play_audio(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        audio_track: &audio::AudioFile,
    ) -> Result<TrackHandle, AudioError>;
}

#[async_trait]
impl SongbirdHelper for Songbird {
    async fn play_audio(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
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
    let connection = ctx.data().db_connection();
    let limit = 5;

    // low char query
    if partial.len() < 3 {
        log::debug!("low character auto complete: '{partial}'");
        let table_name = AudioTable::TABLE_NAME;
        let sql = format!("SELECT name FROM {table_name} ORDER BY created_at DESC LIMIT {limit}");
        let mut stmt = connection
            .prepare(sql.as_str())
            .expect("Autocomplete low-char sql invalid");

        let rows = stmt.query_map((), |row| row.get("name"));
        match rows {
            Ok(rows) => {
                let rows: Vec<String> = rows.filter_map(|row| row.ok()).collect();
                return futures::stream::iter(rows);
            }
            Err(err) => {
                log::error!("Autocomplete low-char sql query error - {err}");
                return futures::stream::iter(vec![]);
            }
        }
    }

    let text = partial.fts_prepare_search();
    let fts5_table_name = db::AudioTable::FTS5_TABLE_NAME;
    let sql = format!("SELECT name FROM {fts5_table_name} WHERE name MATCH '{text}' LIMIT {limit}");
    let mut stmt = connection
        .prepare(sql.as_str())
        .expect("Autocomplete sql invalid");

    let rows = stmt.query_map((), |row| row.get("name"));

    match rows {
        Ok(rows) => {
            let rows: Vec<String> = rows.filter_map(|row| row.ok()).collect();
            futures::stream::iter(rows)
        }
        Err(err) => {
            log::error!("Autocomplete sql query error - {err}");
            futures::stream::iter(vec![])
        }
    }
}
