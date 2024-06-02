use std::fs;

use crate::audio::AudioDir;
use crate::config::Config;
use crate::db::{AudioTable, Connection};

pub struct UserData {
    pub config: Config,
    pub db_pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
}

impl UserData {
    pub fn read_audio_dir(&self) -> AudioDir {
        AudioDir::new(self.config.audio_dir.clone())
    }

    pub fn db_connection(&self) -> Connection {
        self.db_pool
            .get()
            .expect("Failed to get Pooled SQLite connection")
    }
}

pub trait LogResult<E> {
    /// Logs error message as `'{err}'` format, only on Err results. Returns entire Result
    fn log_err(self) -> Self;
    /// Calls op to create message for `log::error!()` only on Err results. Returns entire Result
    fn log_err_op(self, op: impl FnOnce(&E) -> String) -> Self;
    /// Logs error message as `'{msg} - {err}'` format, only on Err results. Returns entire Result
    fn log_err_msg(self, msg: impl AsRef<str>) -> Self;
}

impl<T, E> LogResult<E> for Result<T, E>
where
    E: std::fmt::Display,
{
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
