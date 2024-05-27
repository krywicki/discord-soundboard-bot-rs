use std::path;

use chrono;
use r2d2_sqlite::rusqlite::OptionalExtension;
use r2d2_sqlite::{
    rusqlite::{self, Connection},
    SqliteConnectionManager,
};
use songbird::typemap::TypeMapKey;

use crate::audio;

pub struct DbKey;

impl TypeMapKey for DbKey {
    type Value = r2d2::Pool<SqliteConnectionManager>;
}

pub struct AudioTrack {
    pub id: i64,
    pub name: String,
    pub audio_file: path::PathBuf,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub user_id: Option<u64>,
    pub user_name: Option<String>,
    pub user_global_name: Option<String>,
}

trait Table<'a> {
    const NAME: &'a str;

    fn new(connection: &'a Connection) -> Self;

    fn connection(&self) -> &'a Connection;

    fn create_table(&self);

    fn drop_table(&self) {
        match self
            .connection()
            .execute(format!("DROP TABLE {}", Self::NAME).as_str(), ())
        {
            Ok(_) => log::info!("Dropped table: {}", Self::NAME),
            Err(err) => log::error!("Error dropping table: {} - {}", Self::NAME, err),
        }
    }
}

pub struct AudioTable<'a> {
    conn: &'a Connection,
}

impl<'a> AudioTable<'a> {
    pub const DATETIME_FMT: &str = "%Y-%m-%d %H:%M:%SZ";

    /// Remove rows from table if audio file does not exist.
    /// Adds rows to table for audio files found in directory but not in table.
    pub fn scan_audio_tracks(&self) {
        let mut audio_files = audio::list_audio_track_files();

        // remove audio tracks from table that do not exist in directory
        for row in self.list_audio_rows() {
            let idx = audio_files
                .iter()
                .position(|audio_file| *audio_file == row.audio_file);

            match idx {
                // if audio track in directory AND table, then ignore
                Some(idx) => {
                    audio_files.remove(idx);
                }
                // Audio track missing in directory. Remove row from table
                None => match row.audio_file.to_str() {
                    Some(f) => self.delete_row_by_audio_file(f),
                    None => log::error!(
                        "Failed to convert str into Path for audio_file: {:?} while scanning audio tracks",
                        row.audio_file
                    ),
                },
            }
        }
    }

    pub fn insert_audio_rows(&self, rows: &[AudioTrack]) {
        todo!("insert audio rows")
    }

    pub fn list_audio_rows(&self) -> Vec<AudioTrack> {
        let mut stmt = self
            .conn
            .prepare(
                format!(
                    "SELECT id, name, audio_file, created_at, user_id, user_name, user_global_name
                     FROM {table_name}",
                    table_name = Self::NAME
                )
                .as_str(),
            )
            .unwrap();

        const ID_IDX: usize = 0;
        const NAME_IDX: usize = 1;
        const AUDIO_FILE_IDX: usize = 2;
        const CREATED_AT_IDX: usize = 3;
        const USER_ID_IDX: usize = 4;
        const USER_NAME_IDX: usize = 5;
        const USER_GLOBAL_NAME_IDX: usize = 6;

        let audio_track_iter = stmt
            .query_map([], |row| {
                let audio_file: String = row.get(AUDIO_FILE_IDX).unwrap();
                let audio_file_path = path::Path::new(audio_file.as_str());

                let dt: chrono::DateTime<chrono::Utc> = row.get(CREATED_AT_IDX).unwrap();

                Ok(AudioTrack {
                    id: row.get(ID_IDX).unwrap(),
                    name: row.get(NAME_IDX).unwrap(),
                    audio_file: audio_file_path.to_owned(),
                    created_at: row.get(CREATED_AT_IDX).unwrap(),
                    user_id: row.get(USER_ID_IDX).map_or(None, |id| Some(id)),
                    user_name: row.get(USER_NAME_IDX).map_or(None, |name| Some(name)),
                    user_global_name: row
                        .get(USER_GLOBAL_NAME_IDX)
                        .map_or(None, |global_name| Some(global_name)),
                })
            })
            .unwrap();

        audio_track_iter
            .filter_map(|row| match row {
                Ok(val) => Some(val),
                Err(err) => {
                    log::error!("Failed while iterating on list_audio_rows - {}", err);
                    None
                }
            })
            .collect()
    }

    pub fn has_audio_file(&self, audio_file: &path::PathBuf) -> bool {
        let audio_file = audio_file.to_str().unwrap_or("<?>");

        log::debug!("Checking for existence of audio_file: {}", audio_file);

        let value: rusqlite::Result<String> = self.conn.query_row(
            format!(
                "
                SELECT id FROM {table_name} WHERE audio_file = '{audio_file}'
                ",
                table_name = Self::NAME,
                audio_file = audio_file
            )
            .as_str(),
            (),
            |row| row.get(0),
        );

        match value.optional() {
            Ok(val) => match val {
                Some(v) => {
                    log::debug!("Audio table does not contain audio file: {}", audio_file);
                    true
                }
                None => {
                    log::debug!("Audio table does contain audio file: {}", audio_file);
                    false
                }
            },
            Err(err) => {
                log::error!(
                    "Failed query row on table: {table_name} in has_audio_file",
                    table_name = Self::NAME
                );
                false
            }
        }
    }

    pub fn delete_row_by_audio_file(&self, audio_file: impl AsRef<str>) {
        let audio_file = audio_file.as_ref();
        match self.conn.execute(
            format!(
                "DELETE FROM {table_name} WHERE audio_file = '{audio_file}'",
                table_name = Self::NAME,
                audio_file = audio_file
            )
            .as_str(),
            (),
        ) {
            Ok(_) => {}
            Err(err) => {
                log::error!("Failed to delete row by audio_file = '{}'", audio_file)
            }
        };
    }

    pub fn insert_audio_track(&self, name: impl AsRef<str>, audio_file: impl AsRef<str>) {
        let name = name.as_ref();
        let audio_file = audio_file.as_ref();

        match self.conn.execute(
            format!(
                "
                INSERT INTO {table_name} (name, audio_file) VALUES (?1, ?2)
                ",
                table_name = Self::NAME,
            )
            .as_str(),
            (&name, &audio_file),
        ) {
            Ok(row_count) => log::info!(
                "{} Table: Inserted {} rows - Name: {}, File: {}",
                Self::NAME,
                row_count,
                name,
                audio_file
            ),
            Err(err) => log::error!(
                "Failed to insert audio track into {} Table - {name} - {err}",
                Self::NAME,
            ),
        }
    }
}

impl<'a> Table<'a> for AudioTable<'a> {
    const NAME: &'a str = "audio";

    fn connection(&self) -> &'a Connection {
        &self.conn
    }

    fn new(connection: &'a Connection) -> Self {
        Self { conn: connection }
    }

    fn create_table(&self) {
        match self.conn.execute_batch(
            format!(
                "
                BEGIN;
                    CREATE TABLE IF NOT EXISTS {table_name}(
                        id INTEGER PRIMARY KEY,
                        name VARCHAR(50) NOT NULL UNIQUE,
                        audio_file VARCHAR(500) NOT NULL UNIQUE,
                        created_at VARCHAR(25) NOT NULL,
                        user_id INTEGER,
                        user_name VARCHAR(256),
                        user_global_name VARCHAR(256)
                    );

                    CREATE VIRTUAL TABLE IF NOT EXISTS {fts5_table_name} USING FTS5(
                        name, audio_file, content={table_name}, content_rowid=id
                    );

                    CREATE TRIGGER IF NOT EXISTS {table_name}_insert AFTER INSERT ON {table_name} BEGIN
                        INSERT INTO {fts5_table_name}(rowid, name, audio_file)
                            VALUES (new.id, new.name, new.audio_file);
                    END;

                    CREATE TRIGGER IF NOT EXISTS {table_name}_delete AFTER DELETE ON {table_name} BEGIN
                        INSERT INTO {fts5_table_name}({fts5_table_name}, rowid, name, audio_file)
                            VALUES('delete', old.id, old.name, old.audio_file);
                    END;

                    CREATE TRIGGER {table_name}_update AFTER UPDATE ON {table_name} BEGIN
                        INSERT INTO {fts5_table_name}({fts5_table_name}, rowid, name, audio_file)
                            VALUES('delete', old.id, old.name, old.audio_file);

                        INSERT INTO {fts5_table_name}(rowid, name, audio_file)
                            VALUES (new.id, new.name, new.audio_file);
                    END;
                COMMIT;",
                table_name = Self::NAME,
                fts5_table_name = format!("fts5_{}", Self::NAME)
            )
            .as_str(),
        ) {
            Ok(_) => log::info!("Created table: {}", Self::NAME),
            Err(err) => log::error!("Failed to create table: {} - {}", Self::NAME, err),
        }
    }
}
