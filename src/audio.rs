use std::fs;
use std::path;
use std::sync::Arc;

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
use crate::{helpers, vars};

pub fn find_audio_track(name: &str) -> Option<songbird::input::File<impl AsRef<path::Path>>> {
    let audio_dir: String = vars::env::get(vars::env::DISCORD_BOT_AUDIO_DIR);
    let audio_file = format!("{}.mp3", name);

    let audio_file_path = path::Path::new(&audio_dir).join(&audio_file);

    log::debug!(
        "Looking for audio track: {}",
        audio_file_path.to_str().unwrap_or("")
    );
    if audio_file_path.exists() {
        Some(songbird::input::File::new(audio_file_path))
    } else {
        None
    }
}

pub fn list_audio_track_names() -> Vec<String> {
    let audio_dir: String = vars::env::get(vars::env::DISCORD_BOT_AUDIO_DIR);

    log::debug!("DISCORD_BOT_AUDIO_DIR: {audio_dir}");

    let mut audio_tracks = list_audio_track_files();

    let audio_tracks: Vec<String> = match fs::read_dir(&audio_dir) {
        Ok(entries) => {
            let mut tracks: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| {
                    let p = e.path();
                    let os_path = p.file_name().unwrap();
                    os_path.to_str().unwrap().to_string()
                })
                .filter(|e| e.ends_with(".mp3"))
                .map(|e| e.strip_suffix(".mp3").unwrap().to_string())
                //.map(|e| String::from(e.to_str().unwrap()))
                .collect::<Vec<String>>();
            tracks.sort();
            tracks
        }
        Err(err) => {
            log::error!("Failed to read audio tracks at dir: {audio_dir}");
            vec![]
        }
    };

    log::debug!("Audio tracks: {:?}", audio_tracks);

    audio_tracks
}

pub fn list_audio_track_files() -> Vec<path::PathBuf> {
    let audio_dir: String = vars::env::get(vars::env::DISCORD_BOT_AUDIO_DIR);
    log::debug!("DISCORD_BOT_AUDIO_DIR: {audio_dir}");

    match fs::read_dir(&audio_dir) {
        Ok(entries) => {
            return entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| e.path())
                .filter(|e| e.ends_with(".mp3"))
                .collect();
        }
        Err(err) => {
            log::error!("Failed to read audio tracks at dir: {audio_dir}");
            return Vec::new();
        }
    };
}

pub fn list_audio_track_names_markdown() -> String {
    let audio_track_names = list_audio_track_names();
    let command_prefix: String = vars::env::get(vars::env::DISCORD_BOT_COMMAND_PREFIX);

    let audio_tracks_md = audio_track_names
        .iter()
        .map(|track| format!("- {command_prefix}play {track}\n"))
        .collect();

    audio_tracks_md
}

pub async fn play_audio_track(
    manager: Arc<Songbird>,
    guild_id: GuildId,
    channel_id: ChannelId,
    audio_track: impl AsRef<str>,
) -> Result<TrackHandle, PoiseError> {
    let audio_track = audio_track.as_ref();

    log::debug!("Starting to play_audio_track - {}", &audio_track);

    match manager.get(guild_id) {
        Some(handler_lock) => {
            let mut handler = handler_lock.lock().await;

            match find_audio_track(&audio_track) {
                Some(audio_track_input) => {
                    let track_handle = handler.play_input(audio_track_input.into());
                    log::info!("Playing track {}", audio_track);

                    return Ok(track_handle);
                }
                None => {
                    return Err(AudioError::AudioTrackNotFound {
                        track: audio_track.to_string(),
                    }
                    .into())
                }
            }
        }
        None => Err(AudioError::NotInVoiceChannel { guild_id: guild_id }.into()),
    }
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
