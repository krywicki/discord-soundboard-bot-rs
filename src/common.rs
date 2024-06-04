use std::{fs, path};

use crate::audio::AudioDir;
use crate::config::Config;
use crate::db::{AudioTable, Connection, SettingsTable};

pub struct UserData {
    pub config: Config,
    pub db_pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl UserData {
    pub fn read_audio_dir(&self) -> AudioDir {
        read_audio_dir(&self.config.audio_dir)
    }

    pub fn db_connection(&self) -> Connection {
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
}

pub fn read_audio_dir(dir: &path::PathBuf) -> AudioDir {
    log::debug!("read_audio_dir: {}", dir.to_string_lossy());
    AudioDir::new(dir.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_dir() -> path::PathBuf {
        let dir = std::env::temp_dir();
        let uuid = uuid::Uuid::new_v4();
        let mut encode_buf = uuid::Uuid::encode_buffer();
        let uuid = uuid.hyphenated().encode_lower(&mut encode_buf);
        let temp_dir = dir.join(uuid);
        fs::create_dir(temp_dir.as_path()).expect("Failed creating temp directory");
        temp_dir
    }

    #[test]
    fn read_audio_dir_test() {
        let dir = make_temp_dir();
        std::fs::File::create(dir.join("a.mp3")).unwrap();
        std::fs::File::create(dir.join("b.mp3")).unwrap();
        std::fs::File::create(dir.join("c.txt")).unwrap();

        let audio_dir = read_audio_dir(&dir);

        let audio_tracks: Vec<_> = audio_dir.into_iter().collect();

        assert_eq!(audio_tracks.len(), 2);

        let c_txt = audio_tracks
            .iter()
            .find(|i| i.as_path() == dir.join("c.txt"));
        assert_eq!(c_txt, None);
    }
}
