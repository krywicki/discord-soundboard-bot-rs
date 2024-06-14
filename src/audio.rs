use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path;

use futures::StreamExt;
use rusqlite::types::FromSql;
use rusqlite::ToSql;
use serenity::async_trait;

use songbird::tracks::{PlayMode, TrackHandle};

use symphonia::core::codecs;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::commands::PoiseError;
use crate::common::LogResult;
use crate::helpers::{self, TitleCase};

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

pub struct AudioDir(path::PathBuf);

impl AudioDir {
    pub fn new(dir_path: path::PathBuf) -> Self {
        if !dir_path.is_dir() {
            panic!(
                "Audio directory path is not a directory: {}",
                dir_path.to_str().unwrap_or("")
            );
        }

        Self(dir_path)
    }
}

impl IntoIterator for AudioDir {
    type Item = AudioFile;
    type IntoIter = AudioDirIter;

    fn into_iter(self) -> Self::IntoIter {
        let entries = fs::read_dir(&self.0).expect("Failed to fs::read_dir for AudioDir");
        AudioDirIter(entries.into_iter())
    }
}

pub struct AudioFileValidator {
    max_dur: std::time::Duration,
    reject_uuid_files: bool,
}

impl Default for AudioFileValidator {
    fn default() -> Self {
        Self {
            max_dur: crate::config::default_max_audio_file_duration(),
            reject_uuid_files: true,
        }
    }
}

impl AudioFileValidator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn max_audio_duration(mut self, max_duration: std::time::Duration) -> Self {
        self.max_dur = max_duration;
        self
    }

    pub fn reject_uuid_files(mut self, reject: bool) -> Self {
        self.reject_uuid_files = reject;
        self
    }

    pub fn validate(&self, path: impl AsRef<path::Path>) -> Result<(), PoiseError> {
        let path = path.as_ref();
        log::info!("Validating audio file: {}", path.to_string_lossy());

        if !path.exists() {
            return Err("Audio file path doesn't exist".into()).log_err();
        }

        if !path.is_file() {
            return Err("Audio file path isn't a file".into()).log_err();
        }

        if self.reject_uuid_files {
            let stem = path.file_stem().ok_or("File missing stem")?;
            let stem = stem.to_string_lossy();
            if uuid::Uuid::parse_str(&stem).is_ok() {
                return Err(
                    "Audio file had been added via discord comman, hence the UUID file stem".into(),
                )
                .log_err();
            }
        }

        let track_info = probe_audio_track(&path).log_err()?;
        let track_dur = &track_info.duration;

        if track_dur > &self.max_dur {
            let track_dur = track_dur.as_secs_f64();
            let max_dur = self.max_dur.as_secs_f64();
            return Err(format!("Audio track is {track_dur:.2}s long. This exceeds the max duration of {max_dur:.2}s").into()).log_err();
        }

        Ok(())
    }
}

pub struct AudioDirIter(fs::ReadDir);

impl std::iter::Iterator for AudioDirIter {
    type Item = AudioFile;

    fn next(&mut self) -> Option<Self::Item> {
        let it = &mut self.0;

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
        std::fs::remove_file(self.0.as_path())
            .log_err_msg(format!(
                "Failed to delete audio file {}",
                self.0.to_string_lossy()
            ))
            .ok();
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
        let stem = stem.replace("_", " ").replace("-", " ");

        stem.to_title_case()
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

    let file: fs::File = std::fs::File::open(path).log_err()?;
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
    let format = probed.format;

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

    let track_time_base = track
        .codec_params
        .time_base
        .ok_or("Couldn't find audio track time base")?
        .calc_time(
            track
                .codec_params
                .n_frames
                .ok_or("Couldn't get number of frames for audio track")?,
        );

    let duration_s = track_time_base.seconds as f64 + track_time_base.frac;
    log::info!("Audio track duration = {duration_s:.2}s");
    Ok(AudioTrackInfo {
        duration: std::time::Duration::from_secs_f64(duration_s),
    })
}

/// download audio url to temp dir (audio file is uuid4 name)
pub async fn download_audio_url_temp(url: impl AsRef<str>) -> Result<path::PathBuf, PoiseError> {
    let url = url.as_ref();
    log::info!("Downloading audio url - {url}");

    let client = reqwest::Client::new();

    // HEAD request to ensure Content-Type == 'audio/mpeg'
    let response = client
        .head(url)
        .send()
        .await
        .log_err_msg("Download audio url failed HTTP HEAD")?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_file_test() {
        let f = AudioFile::new(path::PathBuf::from("/tmp/once-Upon a_time.mp3"));
        assert_eq!("Once Upon A Time", f.audio_title());
    }
}
