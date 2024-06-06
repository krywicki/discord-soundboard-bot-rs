use std::borrow::Borrow;
use std::path;

use chrono;
use futures::task;
use r2d2_sqlite::rusqlite::OptionalExtension;
use r2d2_sqlite::{
    rusqlite::{self},
    SqliteConnectionManager,
};
use regex::Regex;
use rusqlite::types::FromSql;
use rusqlite::{MappedRows, Row, ToSql};

use crate::audio;
use crate::commands::PoiseError;
use crate::common::LogResult;

pub struct AudioTableRow {
    pub id: i64,
    pub name: String,
    pub tags: String,
    pub audio_file: audio::AudioFile,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub author_id: Option<u64>,
    pub author_name: Option<String>,
    pub author_global_name: Option<String>,
}

impl TryFrom<&rusqlite::Row<'_>> for AudioTableRow {
    type Error = rusqlite::Error;

    fn try_from(row: &rusqlite::Row) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.get("id").log_err_msg("From row.id fail")?,
            name: row.get("name").log_err_msg("From row.name fail")?,
            tags: row.get("tags").log_err_msg("From row.tags fail")?,
            audio_file: row
                .get("audio_file")
                .log_err_msg("From row.audio_file fail")?,
            created_at: row
                .get("created_at")
                .log_err_msg("From row.created_at fail")?,
            author_id: row
                .get("author_id")
                .log_err_msg("From row.author_id fail")?,
            author_name: row
                .get("author_name")
                .log_err_msg("From row.author_name fail")?,
            author_global_name: row
                .get("author_global_name")
                .log_err_msg("From row.author_global_name fail")?,
        })
    }
}

pub struct AudioTableRowInsert {
    pub name: String,
    pub tags: String,
    pub audio_file: audio::AudioFile,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub author_id: Option<u64>,
    pub author_name: Option<String>,
    pub author_global_name: Option<String>,
}

pub type Connection = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

pub trait FtsText {
    fn fts_clean(&self) -> String;
    fn fts_prepare_search(&self) -> String;
}

impl FtsText for String {
    fn fts_clean(&self) -> String {
        fts_clean_text(&self)
    }

    fn fts_prepare_search(&self) -> String {
        fts_prepare_search(fts_clean_text(&self))
    }
}

impl<'a> FtsText for &'a str {
    fn fts_clean(&self) -> String {
        fts_clean_text(&self)
    }

    fn fts_prepare_search(&self) -> String {
        fts_prepare_search(fts_clean_text(&self))
    }
}

pub fn fts_clean_text(text: impl AsRef<str>) -> String {
    let text = text.as_ref();

    // Convert words like It's -> Its
    let text = text.replace("'", "");

    // Replace all non alphanumeric & space chars with space char
    let re = Regex::new(r"[^a-zA-Z0-9 ]").unwrap();
    let text = re.replace_all(text.as_str(), " ");

    // Remove replace 2x or more space chars to single space char
    let re = Regex::new(r"\s{2,}").unwrap();
    let text = re.replace_all(text.borrow(), " ");

    let text = text.to_lowercase();

    text.trim().into()
}

pub fn fts_prepare_search(text: impl AsRef<str>) -> String {
    let s = text.as_ref();

    s.split_whitespace()
        .map(|word| format!("{word}*"))
        .reduce(|cur, nxt| format!("{cur} {nxt}"))
        .unwrap_or("".into())
}

#[derive(Debug, Clone)]
pub enum UniqueAudioTableCol {
    Id(i64),
    Name(String),
    AudioFile(String),
}

impl AsRef<UniqueAudioTableCol> for UniqueAudioTableCol {
    fn as_ref(&self) -> &UniqueAudioTableCol {
        &self
    }
}

impl UniqueAudioTableCol {
    pub fn sql_condition(&self) -> String {
        match self {
            Self::Id(id) => format!("id = '{id}' "),
            Self::Name(name) => format!("name = '{name}' "),
            Self::AudioFile(audio_file) => format!("audio_file = '{audio_file}' "),
        }
    }
}

pub trait Table {
    fn connection(&self) -> &Connection;
    fn create_table(&self);
    fn drop_table(&self);
}

pub struct AudioTable {
    conn: Connection,
}

impl AudioTable {
    pub const DATETIME_FMT: &str = "%Y-%m-%d %H:%M:%SZ";
    pub const TABLE_NAME: &'static str = "audio";
    pub const FTS5_TABLE_NAME: &str = "fts5_audio";

    pub fn new(connection: Connection) -> Self {
        Self { conn: connection }
    }

    pub fn find_audio_row(&self, col: impl AsRef<UniqueAudioTableCol>) -> Option<AudioTableRow> {
        let col = col.as_ref();
        let table_name = Self::TABLE_NAME;

        let sql_condition = col.sql_condition();
        let sql = format!("SELECT * FROM {table_name} WHERE {sql_condition}");

        self.conn
            .query_row(sql.as_str(), (), |row| AudioTableRow::try_from(row))
            .log_err_msg(format!("Failed to find audio row - {col:?}"))
            .ok()
    }

    pub fn insert_audio_row(&self, audio_row: AudioTableRowInsert) -> Result<(), String> {
        log::info!(
            "Inserting audio row. Name: {}, File: {}",
            audio_row.name,
            audio_row.audio_file.to_string_lossy()
        );
        let table_name = Self::TABLE_NAME;
        let sql = format!(
            "
            INSERT INTO {table_name}
                (name, tags, audio_file, created_at, author_id, author_name, author_global_name)
            VALUES
                (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        );

        let num_inserted = self
            .connection()
            .execute(
                sql.as_str(),
                (
                    &audio_row.name,
                    &audio_row.tags,
                    &audio_row.audio_file,
                    &audio_row.created_at,
                    &audio_row.author_id,
                    &audio_row.author_name,
                    &audio_row.author_global_name,
                ),
            )
            .map_err(|err| {
                log::error!("Failed to insert audio row - {err}");
                err.to_string()
            })?;

        Ok(())
    }

    pub fn has_audio_file(&self, audio_file: &path::PathBuf) -> bool {
        let audio_file = audio_file.to_str().unwrap_or("<?>");

        log::debug!("Checking for existence of audio_file: {}", audio_file);

        let value: rusqlite::Result<String> = self.conn.query_row(
            format!(
                "
                SELECT id FROM {table_name} WHERE audio_file = '{audio_file}'
                ",
                table_name = Self::TABLE_NAME,
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
                    table_name = Self::TABLE_NAME
                );
                false
            }
        }
    }

    pub fn update_audio_row(&self, audio_row: &AudioTableRow) -> Result<(), String> {
        log::info!("Updating audio row. Name: {}", audio_row.name);

        let table_name = Self::TABLE_NAME;
        let name = &audio_row.name;
        let tags = &audio_row.tags;
        let row_id = audio_row.id;

        let sql = format!(
            "
            UPDATE {table_name}
            SET
                name = '{name}',
                tags = '{tags}'
            WHERE
                id = {row_id};
        "
        );

        self.conn
            .execute(sql.as_str(), ())
            .log_err_msg("Failed updating audio track")
            .map_err(|err| err.to_string())?;

        log::info!("Updated audio row. Name: {name}");
        Ok(())
    }

    pub fn delete_audio_row(&self, col: impl AsRef<UniqueAudioTableCol>) -> Result<(), PoiseError> {
        let column = col.as_ref();
        match self.find_audio_row(&col) {
            None => log::info!("Can't delete non-existent audio track. {column:?}"),
            Some(row) => {
                row.audio_file.delete();
                let table_name = Self::TABLE_NAME;
                let row_id = row.id;
                let sql = format!("DELETE FROM {table_name} WHERE id = {row_id}");

                self.conn
                    .execute(sql.as_str(), ())
                    .log_err_msg("Failed to delete audio row")?;
            }
        }
        Ok(())
    }
}

impl Table for AudioTable {
    fn connection(&self) -> &Connection {
        &self.conn
    }

    fn drop_table(&self) {
        let table_name = Self::TABLE_NAME;
        let fts5_table_name = Self::FTS5_TABLE_NAME;
        let sql = format!(
            "
            BEGIN TRANSACTION
                DROP TABLE {fts5_table_name};
                DROP TABLE {table_name};
            COMMIT;
        "
        );
        self.connection()
            .execute_batch(sql.as_str())
            .log_err_msg(format!(
                "Failed dropping tables: {table_name}, {fts5_table_name}"
            ))
            .log_ok_msg(format!("Dropped tables: {table_name}, {fts5_table_name}"));
    }

    fn create_table(&self) {
        let table_name = Self::TABLE_NAME;
        let fts5_table_name = Self::FTS5_TABLE_NAME;

        log::info!("Creating tables {table_name}, {fts5_table_name}...");

        let sql = format!(
            "
            BEGIN;
                CREATE TABLE IF NOT EXISTS {table_name} (
                    id INTEGER PRIMARY KEY,
                    name VARCHAR(80) NOT NULL UNIQUE,
                    tags VARCHAR(2048) NOT NULL,
                    audio_file VARCHAR(500) NOT NULL UNIQUE,
                    created_at VARCHAR(25) NOT NULL,
                    author_id INTEGER,
                    author_name VARCHAR(256),
                    author_global_name VARCHAR(256)
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS {fts5_table_name} USING FTS5(
                    name, tags, content={table_name}, content_rowid=id
                );

                CREATE TRIGGER IF NOT EXISTS {table_name}_insert AFTER INSERT ON {table_name} BEGIN
                    INSERT INTO {fts5_table_name}(rowid, name, tags)
                        VALUES (new.id, new.name, new.tags);
                END;

                CREATE TRIGGER IF NOT EXISTS {table_name}_delete AFTER DELETE ON {table_name} BEGIN
                    INSERT INTO {fts5_table_name}({fts5_table_name}, rowid, name, tags)
                        VALUES('delete', old.id, old.name, old.tags);
                END;

                CREATE TRIGGER IF NOT EXISTS {table_name}_update AFTER UPDATE ON {table_name} BEGIN
                    INSERT INTO {fts5_table_name}({fts5_table_name}, rowid, name, tags)
                        VALUES('delete', old.id, old.name, old.tags);

                    INSERT INTO {fts5_table_name}(rowid, name, tags)
                        VALUES (new.id, new.name, new.tags);
                END;
            COMMIT;"
        );

        self.conn
            .execute_batch(sql.as_str())
            .log_err_msg(format!("Failed creating table:{table_name}"))
            .unwrap();

        log::info!("Created tables {table_name}, {fts5_table_name}!");
    }
}

pub struct SettingsTableRow {
    pub id: i64,
    pub join_audio: Option<String>,
    pub leave_audio: Option<String>,
}

impl TryFrom<&rusqlite::Row<'_>> for SettingsTableRow {
    type Error = rusqlite::Error;

    fn try_from(row: &rusqlite::Row<'_>) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.get("id")?,
            join_audio: row.get("join_audio")?,
            leave_audio: row.get("leave_audio")?,
        })
    }
}
pub struct SettingsTable {
    conn: Connection,
}

impl SettingsTable {
    const TABLE_NAME: &str = "settings";

    pub fn new(connection: Connection) -> Self {
        Self { conn: connection }
    }

    fn first_row(&self) -> Result<Option<SettingsTableRow>, PoiseError> {
        let table_name = Self::TABLE_NAME;
        let sql = format!("SELECT * FROM {table_name} LIMIT 1");
        Ok(self
            .conn
            .query_row(sql.as_str(), (), |row| SettingsTableRow::try_from(row))
            .optional()
            .log_err_msg(format!("Failed to get first row of {table_name}"))?)
    }

    fn init_settings(&self) -> Result<SettingsTableRow, PoiseError> {
        let table_name = Self::TABLE_NAME;

        let sql = format!(
            "
            INSERT INTO {table_name}
                (join_audio, leave_audio)
            VALUES
                (?1, ?2)
            "
        );

        let none: Option<String> = None;
        self.conn
            .execute(sql.as_str(), (&none, &none))
            .log_err_msg(format!("Failed init settings row in table: {table_name}"))?;

        Ok(self
            .first_row()
            .log_err()?
            .ok_or("Failed to insert initial settings row")?)
    }

    pub fn get_settings(&self) -> Result<SettingsTableRow, PoiseError> {
        match self.first_row()? {
            Some(settings) => Ok(settings),
            None => self.init_settings(),
        }
    }

    pub fn update_settings(&self, settings: &SettingsTableRow) -> Result<(), PoiseError> {
        log::info!("Saving settings");

        let table_name = Self::TABLE_NAME;
        let row_id = settings.id;
        let join_audio = settings
            .join_audio
            .as_ref()
            .map_or("NULL".into(), |val| format!("'{val}'"));
        let leave_audio = settings
            .leave_audio
            .as_ref()
            .map_or("NULL".into(), |val| format!("'{val}'"));

        let sql = format!(
            "
            UPDATE {table_name}
            SET
                join_audio = {join_audio},
                leave_audio = {leave_audio}
            WHERE
                id = {row_id};
            "
        );

        self.conn.execute(sql.as_str(), ()).log_err()?;

        Ok(())
    }
}

impl Table for SettingsTable {
    fn connection(&self) -> &Connection {
        &self.conn
    }

    fn create_table(&self) {
        let table_name = Self::TABLE_NAME;
        log::info!("Creating table: {table_name}");
        let sql = format!(
            "
            CREATE TABLE IF NOT EXISTS {table_name} (
                id INTEGER PRIMARY KEY,
                join_audio VARCHAR(80),
                leave_audio VARCHAR(80)
            );
        "
        );

        self.conn
            .execute_batch(sql.as_str())
            .log_err_msg("Failed create table")
            .log_ok_msg(format!("Created table {table_name}"))
            .unwrap();
    }

    fn drop_table(&self) {
        let table_name = Self::TABLE_NAME;
        log::info!("Dropping table: {table_name}");

        let sql = format!("DROP TABLE {table_name}");
        self.conn
            .execute(sql.as_str(), ())
            .log_err_msg("Failed to drop table")
            .log_ok_msg(format!("Dropped table {table_name}"));
    }
}

#[derive(Debug)]
pub enum AudioTableOrderBy {
    CreatedAt,
    Id,
    Name,
}

impl AudioTableOrderBy {
    pub fn col_name(&self) -> String {
        match &self {
            Self::CreatedAt => "created_at".into(),
            Self::Id => "id".into(),
            Self::Name => "name".into(),
        }
    }
}

#[derive(Debug)]
pub struct AudioTablePaginator {
    conn: Connection,
    order_by: AudioTableOrderBy,
    page_limit: u64,
    offset: u64,
}

impl AudioTablePaginator {
    pub fn builder(conn: Connection) -> AudioTablePaginatorBuilder {
        AudioTablePaginatorBuilder::new(conn)
    }

    pub fn next_page(&mut self) -> Result<Vec<AudioTableRow>, String> {
        let conn = &self.conn;
        let table_name = AudioTable::TABLE_NAME;
        let order_by = self.order_by.col_name();
        let page_limit = self.page_limit;
        let offset = self.offset;

        let sql = format!(
            "SELECT * FROM {table_name}
            ORDER BY {order_by}
            LIMIT {page_limit}
            OFFSET {offset};"
        );

        let mut stmt = conn
            .prepare(sql.as_ref())
            .expect("Failed to prepare sql stmt");

        let row_iter = stmt
            .query_map([], |row| AudioTableRow::try_from(row))
            .map_err(|err| format!("Error in AudioTablePaginator - {err}"))?;

        self.offset += self.page_limit;

        Ok(row_iter
            .filter_map(|row| match row {
                Ok(val) => Some(val),
                Err(err) => {
                    log::error!("{err}");
                    None
                }
            })
            .collect())
    }
}

pub struct AudioTablePaginatorBuilder {
    conn: Connection,
    order_by: AudioTableOrderBy,
    page_limit: u64,
}

impl AudioTablePaginatorBuilder {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: conn,
            order_by: AudioTableOrderBy::Id,
            page_limit: 500,
        }
    }

    pub fn order_by(mut self, value: AudioTableOrderBy) -> Self {
        self.order_by = value;
        self
    }

    pub fn page_limit(mut self, value: u64) -> Self {
        self.page_limit = value;
        self
    }

    pub fn build(self) -> AudioTablePaginator {
        AudioTablePaginator {
            conn: self.conn,
            order_by: self.order_by,
            page_limit: self.page_limit,
            offset: 0,
        }
    }
}

impl Iterator for AudioTablePaginator {
    type Item = Result<Vec<AudioTableRow>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        let rows = self.next_page();
        let mut is_empty = false;

        match rows {
            Ok(ref _rows) => {
                if _rows.is_empty() {
                    return None;
                } else {
                    return Some(rows);
                }
            }

            Err(err) => {
                log::error!("AudiotablePaginator error - {err}");
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::helpers::{self, uuid_v4_str};
    use audio::AudioFile;

    use super::*;

    #[test]
    fn fts_clean_text_test() {
        assert_eq!("i love star wars", fts_clean_text("I love star-wars!  "));

        assert_eq!(
            "i think its borked",
            fts_clean_text("I think it's borked!?!?!?!?")
        );

        assert_eq!(
            "i like code",
            fts_clean_text(r"I like !@#$%^&*(_){}[]/\., code")
        );

        assert_eq!(
            "this is a single line",
            fts_clean_text("This\nis\na\nsingle\nline\n")
        );
    }

    fn get_db_connection() -> Connection {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::new(db_manager).unwrap();
        db_pool.get().unwrap()
    }

    fn get_audio_table() -> AudioTable {
        AudioTable::new(get_db_connection())
    }

    fn make_audio_table_row_insert() -> AudioTableRowInsert {
        AudioTableRowInsert {
            audio_file: AudioFile::new(
                path::Path::new(&format!("/tmp/{}.mp3", helpers::uuid_v4_str())).to_path_buf(),
            ),
            author_global_name: None,
            name: uuid_v4_str().into(),
            tags: uuid_v4_str().into(),
            created_at: chrono::Utc::now(),
            author_id: None,
            author_name: None,
        }
    }

    #[test]
    fn audio_table_create_test() {
        let table = get_audio_table();
        table.create_table(); // create table(s) & trigger(s)
        table.create_table(); // ignore table(s) & triggers(s) already created
    }

    #[test]
    fn audio_table_drop_test() {
        let table = get_audio_table();

        table.drop_table(); // if no table(s) exist
        table.create_table(); // make table(s)
        table.drop_table(); // drop tables
    }

    #[test]
    fn audio_table_insert_row_test() {
        let table = get_audio_table();

        table.create_table();
        table
            .insert_audio_row(make_audio_table_row_insert())
            .unwrap();
    }

    #[test]
    fn audio_table_get_row_test() {
        let table = get_audio_table();
        table.create_table();

        let mut row_insert = make_audio_table_row_insert();
        row_insert.name = "Test".into();
        table.insert_audio_row(row_insert);

        let row = table.find_audio_row(UniqueAudioTableCol::Name("Test".into()));
        let row = row.unwrap();
        assert_eq!(row.name, "Test".to_string());
    }

    #[test]
    fn audio_table_pagination_test() {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::new(db_manager).unwrap();
        let table = AudioTable::new(db_pool.get().unwrap());
        table.create_table();

        for _ in 0..3 {
            table
                .insert_audio_row(make_audio_table_row_insert())
                .unwrap();
        }

        let mut paginator = AudioTablePaginator::builder(db_pool.get().unwrap())
            .page_limit(2)
            .build();

        let page = paginator.next().unwrap().unwrap();
        assert_eq!(page.len(), 2);

        let page = paginator.next().unwrap().unwrap();
        assert_eq!(page.len(), 1);

        let page = paginator.next();
        assert!(page.is_none());
    }

    fn get_settings_table() -> SettingsTable {
        let connection = get_db_connection();
        SettingsTable::new(connection)
    }

    #[test]
    fn settings_table_create_test() {
        let table = get_settings_table();
        table.create_table();
        table.create_table();
    }

    #[test]
    fn settings_table_drop_test() {
        let table = get_settings_table();
        table.drop_table();
        table.create_table();
        table.drop_table();
    }

    #[test]
    fn get_settings_test() {
        let table = get_settings_table();
        table.create_table();
        let settings = table.get_settings().unwrap();

        assert!(settings.join_audio.is_none());
        assert!(settings.leave_audio.is_none());
    }

    #[test]
    fn update_settings_test() {
        let table = get_settings_table();
        table.create_table();
        let mut settings = table.get_settings().unwrap();

        let join_audio = Some("join.mp3".into());
        let leave_audio = Some("leave.mp3".into());

        settings.join_audio = join_audio.clone();
        settings.leave_audio = leave_audio.clone();

        table.update_settings(&settings).unwrap();

        let settings = table.get_settings().unwrap();
        assert_eq!(settings.join_audio, join_audio);
        assert_eq!(settings.leave_audio, leave_audio);
    }
}
