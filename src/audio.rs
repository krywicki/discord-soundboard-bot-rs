use std::ffi::OsStr;
use std::fs;
use std::ops::Deref;
use std::path;
use std::sync::Arc;

use rusqlite::types::FromSql;
use rusqlite::ToSql;
use serenity::async_trait;
use serenity::{
    all::{ChannelId, GuildId},
    client::Context,
};
use songbird::tracks::{PlayMode, TrackHandle};
use songbird::Songbird;

use crate::commands::PoiseContext;
use crate::commands::PoiseError;
use crate::errors::AudioError;
use crate::helpers;

pub async fn play_audio_track(
    manager: Arc<Songbird>,
    guild_id: GuildId,
    channel_id: ChannelId,
    audio_track: impl AsRef<str>,
) -> Result<TrackHandle, PoiseError> {
    Err(AudioError::AudioTrackNotFound {
        track: "?".to_string(),
    }
    .into())
    // let audio_track = audio_track.as_ref();

    // log::debug!("Starting to play_audio_track - {}", &audio_track);

    // match manager.get(guild_id) {
    //     Some(handler_lock) => {
    //         let mut handler = handler_lock.lock().await;

    //         match find_audio_track(&audio_track) {
    //             Some(audio_track_input) => {
    //                 let track_handle = handler.play_input(audio_track_input.into());
    //                 log::info!("Playing track {}", audio_track);

    //                 return Ok(track_handle);
    //             }
    //             None => {
    //                 return Err(AudioError::AudioTrackNotFound {
    //                     track: audio_track.to_string(),
    //                 }
    //                 .into())
    //             }
    //         }
    //     }
    //     None => Err(AudioError::NotInVoiceChannel { guild_id: guild_id }.into()),
    // }
}

pub async fn wait_for_audio_track_end(track_handle: &TrackHandle) {
    loop {
        match track_handle.get_info().await {
            Ok(state) => match state.playing {
                PlayMode::Play => tokio::time::sleep(tokio::time::Duration::from_millis(250)).await,
                _ => {}
            },
            Err(_) => break,
        }
    }
}

#[async_trait]
pub trait TrackHandleHelper {
    async fn wait_for_end(&self);
}

#[async_trait]
impl TrackHandleHelper for TrackHandle {
    async fn wait_for_end(&self) {
        wait_for_audio_track_end(&self);
    }
}

pub struct AudioDir {
    dir_path: path::PathBuf,
}

impl AudioDir {
    pub fn new(dir_path: path::PathBuf) -> Self {
        if !dir_path.is_dir() {
            panic!(
                "Audio directory path is not a directory: {}",
                dir_path.to_str().unwrap_or("")
            );
        }

        Self { dir_path: dir_path }
    }
}

impl IntoIterator for AudioDir {
    type Item = AudioFile;
    type IntoIter = AudioDirIter;

    fn into_iter(self) -> Self::IntoIter {
        let entries = fs::read_dir(&self.dir_path).expect("Failed to fs::read_dir for AudioDir");
        AudioDirIter(entries)
    }
}

pub struct AudioDirIter(fs::ReadDir);

impl std::iter::Iterator for AudioDirIter {
    type Item = AudioFile;

    fn next(&mut self) -> Option<Self::Item> {
        let mut it = &mut self.0;

        it.filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file() && entry.path().ends_with(".mp3"))
            .map(|e| AudioFile(e.path()))
            .next()
    }
}

#[derive(Debug, PartialEq)]
pub struct AudioFile(path::PathBuf);

impl AudioFile {
    /// get file name without file extension
    pub fn file_stem(&self) -> String {
        self.0
            .file_stem()
            .unwrap_or(&OsStr::new(""))
            .to_string_lossy()
            .into()
    }

    pub fn audio_title(&self) -> String {
        let stem = self.file_stem();
        stem.replace("_", " ").replace("-", " ")
    }
}

impl Deref for AudioFile {
    type Target = path::PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Into<songbird::input::File<path::PathBuf>> for AudioFile {
    fn into(self) -> songbird::input::File<path::PathBuf> {
        songbird::input::File::new(self.0)
    }
}

impl FromSql for AudioFile {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            rusqlite::types::ValueRef::Text(val) => {
                let val = String::from_utf8_lossy(val);
                let p = path::PathBuf::from(val.to_string());
                Ok(AudioFile(p))
            }
            _ => Err(rusqlite::types::FromSqlError::InvalidType),
        }
    }
}

impl ToSql for AudioFile {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let p = self.to_str().unwrap_or("");
        let value = rusqlite::types::ValueRef::Text(p.as_bytes());
        Ok(rusqlite::types::ToSqlOutput::Borrowed(value))
    }
}

pub trait RemoveAudioFile {
    fn remove_audio_file(&mut self, audio_file: &AudioFile);
}

impl RemoveAudioFile for Vec<AudioFile> {
    fn remove_audio_file(&mut self, audio_file: &AudioFile) {
        if let Some(index) = self.iter().position(|f| f == audio_file) {
            self.remove(index);
        }
    }
}
