use std::path;

use crate::audio::AudioFile;
use crate::commands::PoiseError;
use crate::config::Config;
use crate::db::{AudioTable, DbConnection, SettingsTable};

pub struct UserData {
    pub config: Config,
    pub db_pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl UserData {
    pub fn db_connection(&self) -> DbConnection {
        self.db_pool
            .get()
            .expect("Failed to get Pooled SQLite connection")
    }

    pub fn audio_table(&self) -> AudioTable {
        AudioTable::new(self.db_connection())
    }

    pub fn settings_table(&self) -> SettingsTable {
        SettingsTable::new(self.db_connection())
    }

    /// Attempts to move file to audio dir. Will attempt copy if move fails
    /// Moves can fail if target file and destination audio directory are on separate partitions of file systems
    pub fn move_file_to_audio_dir(
        &self,
        path: impl AsRef<path::Path>,
    ) -> Result<AudioFile, PoiseError> {
        let target_file = path.as_ref();
        let audio_dir = &self.config.audio_dir;

        log::info!(
            "Move file: {} to audio dir: {}",
            target_file.to_string_lossy(),
            audio_dir.to_string_lossy()
        );

        if !target_file.exists() {
            return Err("Target file doesn't exist".into()).log_err();
        }

        if !target_file.is_file() {
            return Err("Target file is not a file.".into()).log_err();
        }

        let target_file_name = target_file
            .file_name()
            .ok_or("Failed to get target path file name")
            .log_err()?;
        let dest_file = audio_dir.join(target_file_name);

        match std::fs::rename(target_file, &dest_file) {
            Ok(_) => Ok(AudioFile::new(dest_file)),
            Err(err) => {
                log::error!(
                    "Failed to move target file to audio dir - {err}. Attempting copy instead."
                );
                std::fs::copy(target_file, &dest_file)
                    .log_err_msg("Failed to copy target file to audio dir")?;

                log::info!(
                    "Copied target file: {} to destination: {}",
                    target_file.to_string_lossy(),
                    dest_file.to_string_lossy()
                );

                Ok(AudioFile::new(dest_file))
            }
        }
    }
}

pub trait LogResult<T, E> {
    /// Logs error message as `'{err}'` format, only on Err results. Returns Result
    fn log_err(self) -> Self;
    /// Calls op to create message for `log::error!()` only on Err results. Returns Result
    fn log_err_op(self, op: impl FnOnce(&E) -> String) -> Self;
    /// Logs error message as `'{msg} - {err}'` format, only on Err results. Returns Result
    fn log_err_msg(self, msg: impl AsRef<str>) -> Self;
    /// Logs  ok message as `'{msg}'` format, only on Ok results. Returns Result
    fn log_ok_msg(self, msg: impl AsRef<str>) -> Self;
    /// Calls op to create message for `log::info!()` only on Ok results. Returns Result
    #[allow(unused)]
    fn log_ok_op(self, op: impl FnOnce(&T) -> String) -> Self;
}

impl<T, E> LogResult<T, E> for Result<T, E>
where
    E: std::fmt::Display,
{
    fn log_ok_msg(self, msg: impl AsRef<str>) -> Self {
        let msg = msg.as_ref();
        match &self {
            Ok(_) => log::info!("{msg}"),
            _ => {}
        }
        self
    }

    fn log_ok_op(self, op: impl FnOnce(&T) -> String) -> Self {
        match &self {
            Ok(val) => {
                let msg = op(val);
                log::error!("{msg}");
            }
            _ => {}
        }
        self
    }

    fn log_err_msg(self, msg: impl AsRef<str>) -> Self {
        match &self {
            Ok(_) => {}
            Err(err) => {
                let msg = msg.as_ref();
                log::error!("{msg} - {err}");
            }
        }

        self
    }

    fn log_err_op(self, op: impl FnOnce(&E) -> String) -> Self {
        match &self {
            Ok(_) => {}
            Err(err) => {
                let message = op(&err);
                log::error!("{message}");
            }
        }
        self
    }

    fn log_err(self) -> Self {
        match &self {
            Ok(_) => {}
            Err(err) => {
                log::error!("{err}");
            }
        }
        self
    }
}

#[derive(Debug, poise::Modal)]
#[name = "Search Sound"]
pub struct SearchSoundModal {
    #[name = "Search"] // Field name by default
    #[placeholder = "star wars anakin"] // No placeholder by default
    #[min_length = 3] // No length restriction by default (so, 1-4000 chars)
    #[max_length = 80] // Same as max button label len (crate::vars::BTN_LABEL_MAX_LEN)
    name: String,
}
