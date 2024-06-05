use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path;
use std::sync::Arc;

use chrono::{Duration, TimeDelta};
use futures::StreamExt;
use rusqlite::types::FromSql;
use rusqlite::ToSql;
use serenity::async_trait;
use serenity::{
    all::{ChannelId, GuildId},
    client::Context,
};
use songbird::tracks::{PlayMode, TrackHandle};
use songbird::Songbird;
use symphonia::core::codecs::{self, CodecType, DecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::commands::PoiseError;
use crate::commands::{PoiseContext, PoiseResult};
use crate::common::LogResult;
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
}

pub async fn wait_for_audio_track_end(track_handle: &TrackHandle) {
    loop {
        match track_handle.get_info().await {
            Ok(state) => match state.playing {
                PlayMode::Play => tokio::time::sleep(tokio::time::Duration::from_millis(250)).await,
                _ => {}
            },
            Err(err) => {
                log::error!("Error waiting for audio track end - {err}");
                break;
            }
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
        wait_for_audio_track_end(&self).await;
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
        AudioDirIter(entries.into_iter())
    }
}

pub struct AudioDirIter(fs::ReadDir);

impl std::iter::Iterator for AudioDirIter {
    type Item = AudioFile;

    fn next(&mut self) -> Option<Self::Item> {
        let mut it = &mut self.0;

        it.filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_file())
            .filter(|entry| entry.path().extension().unwrap_or(OsStr::new("")) == "mp3")
            .map(|e| AudioFile(e.path()))
            .next()
    }
}

#[derive(Debug, PartialEq)]
pub struct AudioFile(path::PathBuf);

impl AudioFile {
    pub fn new(p: path::PathBuf) -> Self {
        Self(p)
    }

    pub fn delete(&self) {
        std::fs::remove_file(self.0.as_path()).log_err_msg(format!(
            "Failed to delete audio file {}",
            self.0.to_string_lossy()
        ));
    }

    pub fn as_path_buf(&self) -> path::PathBuf {
        self.0.clone()
    }

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

pub struct AudioTrackInfo {
    pub duration: std::time::Duration,
}
pub fn probe_audio_track(audio_file: impl AsRef<path::Path>) -> Result<AudioTrackInfo, PoiseError> {
    let path = audio_file.as_ref();

    log::info!("Probing audio-track: {}", path.to_string_lossy());

    let file = std::fs::File::open(path).log_err()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::default();
    hint.with_extension("mp3");

    // Use the default probe to identify the format
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .log_err_msg("Failed to probe format")?;

    // Get the format reader
    let mut format = probed.format;

    // Get the default track
    let track = format
        .default_track()
        .ok_or("No audio track found")
        .log_err()?;

    if track.codec_params.codec != codecs::CODEC_TYPE_MP3 {
        return Err(format!(
            "Invalid audio codec detected. Expected MP3({}), found {}",
            codecs::CODEC_TYPE_MP3,
            track.codec_params.codec
        )
        .into())
        .log_err();
    }

    // Create a decoder for the track
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .log_err_msg("Failed to create audio decoder")?;

    // Decode the packets and calculate the duration
    let mut duration: f64 = 0.0;
    while let Ok(packet) = format.next_packet() {
        // Decode the packet
        if let Ok(audio_buffer) = decoder.decode(&packet) {
            // Add the duration of this packet
            duration += audio_buffer.frames() as f64 / audio_buffer.spec().rate as f64;
        }
    }

    let duration_ms = (duration * 1000.0) as u64;
    log::info!("Audio track duration = {duration:.2} seconds");
    Ok(AudioTrackInfo {
        duration: std::time::Duration::from_millis(duration_ms),
    })
}

/// download audio url to temp dir (audio file is uuid4 name)
pub async fn download_audio_url_temp(url: impl AsRef<str>) -> Result<path::PathBuf, PoiseError> {
    let url = url.as_ref();
    log::info!("Downloading audio url - {url}");

    let client = reqwest::Client::new();

    // HEAD request to ensure Content-Type == 'audio/mpeg'
    let resp = client
        .head(url)
        .send()
        .await
        .log_err_msg("Download audio url failed HTTP HEAD")?;

    let response = reqwest::get(url).await?;
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .unwrap();

    match content_type.to_str().unwrap_or("") {
        "audio/mpeg" | "audio/mpeg3" | "x-mpeg-3" => {}
        val => {
            return Err(
                format!("Invalid content type: {val} for url. Expected 'audio/mpeg'",).into(),
            )
            .log_err();
        }
    }

    let uuid = helpers::uuid_v4_str();
    let file_name = format!("{uuid}.mp3");
    let audio_file_path = std::env::temp_dir().join(file_name.as_str());

    // Download audio file
    let mut file = std::fs::File::create(audio_file_path.as_path())?;
    let response = client
        .get(url)
        .send()
        .await
        .log_err_msg("Failed HTTP GET on url")?;

    let mut stream = response.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item
            .or(Err(format!("Error while downloading file")))
            .log_err()?;

        file.write_all(&chunk)
            .or(Err(format!("Error while writing to file")))
            .log_err()?;
    }

    Ok(audio_file_path)

    // let track_info = audio::probe_audio_track(&audio_file_path)?;
    // if track_info.duration >= Duration::seconds(7) {
    //     return Err(format!(
    //         "Audio track is too long: {:.2} seconds. Max allowed duration is {} seconds",
    //         (track_info.duration.num_milliseconds() as f64) / 1000.0,
    //         7,
    //     ))
    //     .log_err()?;
    // }

    // // move audio file to destination directory

    // let final_audio_file_path = dest_dir.join(file_name.as_str());
    // std::fs::copy(&audio_file_path, &final_audio_file_path)
    //     .log_err_msg("Could not copy audio file to target dir")
    //     .or(Err("Could not set audio file in target directory"))?;
    // Ok(AudioFile::new(final_audio_file_path))
}

pub struct AudioFileValidator {
    max_audio_file_duration: std::time::Duration,
}

impl Default for AudioFileValidator {
    fn default() -> Self {
        Self {
            max_audio_file_duration: crate::config::default_max_audio_file_duration(),
        }
    }
}

impl AudioFileValidator {
    pub fn max_audio_file_duration(mut self, duration: std::time::Duration) -> Self {
        self.max_audio_file_duration = duration;
        self
    }

    pub fn validate_audio_file(&self, path: impl AsRef<path::Path>) -> Result<(), PoiseError> {
        let audio_file = path.as_ref();
        log::info!("Validating audio file: {}", audio_file.to_string_lossy());

        if !audio_file.exists() {
            return Err("Audio file doesn't exist".into()).log_err();
        }

        if !audio_file.is_file() {
            return Err("Audio file path isn't a file".into()).log_err();
        }

        let track_info = probe_audio_track(&audio_file)?;
        if track_info.duration > self.max_audio_file_duration {
            let track_dur = track_info.duration.as_secs_f64();
            let max_dur = self.max_audio_file_duration.as_secs_f64();
            return Err(format!("Audio track is {track_dur:.2}s long. This exceeds the max duration of {max_dur:.2}s").into()).log_err();
        }

        Ok(())
    }
}
