use std::ops::Deref;

use regex::Regex;
use rusqlite::{params, types::FromSql, ToSql};

use crate::{audio, commands::PoiseError, common::LogResult, db::Order};

use super::{DbConnection, Table};

pub struct AudioTableRow {
    pub id: i64,
    pub name: String,
    pub tags: Tags,
    pub audio_file: audio::AudioFile,
    #[allow(dead_code)]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[allow(dead_code)]
    pub author_id: Option<u64>,
    #[allow(dead_code)]
    pub author_name: Option<String>,
    #[allow(dead_code)]
    pub author_global_name: Option<String>,
    #[allow(dead_code)]
    pub play_count: i64,
    #[allow(dead_code)]
    pub last_played_at: Option<chrono::DateTime<chrono::Utc>>,
    #[allow(dead_code)]
    pub popularity: f64,
    pub pinned: bool,
}

pub struct Tags(Vec<String>);

impl Tags {
    pub fn new() -> Self {
        Tags(vec![])
    }

    pub fn clean_tag(value: impl AsRef<str>) -> String {
        let text = value.as_ref();
        let re = Regex::new(r"[^a-zA-Z0-9-_\s]").unwrap();
        let text = re.replace_all(text, " ");
        let text = text.trim();

        text.into()
    }

    #[allow(dead_code)]
    pub fn inner(&self) -> &Vec<String> {
        &self.0
    }
}

impl Deref for Tags {
    type Target = Vec<String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ToString for Tags {
    fn to_string(&self) -> String {
        self.join(" ")
    }
}

impl From<&str> for Tags {
    fn from(value: &str) -> Self {
        let tags = value.split_whitespace().map(Self::clean_tag).collect();
        Tags(tags)
    }
}

impl From<String> for Tags {
    fn from(value: String) -> Self {
        Tags::from(value.as_str())
    }
}

impl From<Vec<String>> for Tags {
    fn from(value: Vec<String>) -> Self {
        Tags(value)
    }
}

impl ToSql for Tags {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self.len() {
            0 => rusqlite::types::Null.to_sql(),
            _ => Ok(rusqlite::types::ToSqlOutput::Owned(self.to_string().into())),
        }
    }
}

impl FromSql for Tags {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str_or_null()? {
            Some(val) => Ok(Tags::from(val)),
            None => Ok(Tags::new()),
        }
    }
}

impl AsRef<AudioTableRow> for AudioTableRow {
    fn as_ref(&self) -> &AudioTableRow {
        &self
    }
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
            play_count: row
                .get("play_count")
                .log_err_msg("From row.play_count fail")?,
            last_played_at: row
                .get("last_played_at")
                .log_err_msg("From row.last_played_at fail")?,
            popularity: row
                .get("popularity")
                .log_err_msg("From row.popularity fail")?,
            pinned: row.get("pinned").log_err_msg("From row.pinned fail")?,
        })
    }
}

pub struct AudioTableRowInsert {
    pub name: String,
    pub tags: Tags,
    pub audio_file: audio::AudioFile,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub author_id: Option<u64>,
    pub author_name: Option<String>,
    pub author_global_name: Option<String>,
    pub play_count: i64,
    pub last_played_at: Option<chrono::DateTime<chrono::Utc>>,
    pub popularity: f64,
    pub pinned: bool,
}

pub struct AudioTableRowInsertBuilder {
    row_insert: AudioTableRowInsert,
}

impl AudioTableRowInsertBuilder {
    pub fn new(name: impl AsRef<str>, audio_file: audio::AudioFile) -> Self {
        Self {
            row_insert: AudioTableRowInsert {
                name: name.as_ref().into(),
                tags: Tags::new(),
                audio_file: audio_file,
                created_at: chrono::Utc::now(),
                author_id: None,
                author_name: None,
                author_global_name: None,
                play_count: 0,
                last_played_at: None,
                popularity: 0.0,
                pinned: false,
            },
        }
    }

    #[allow(dead_code)]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.row_insert.name = name.into();
        self
    }

    pub fn tags(mut self, tags: impl Into<Tags>) -> Self {
        self.row_insert.tags = tags.into();
        self
    }

    #[allow(dead_code)]
    pub fn audio_file(mut self, audio_file: audio::AudioFile) -> Self {
        self.row_insert.audio_file = audio_file;
        self
    }

    #[allow(dead_code)]
    pub fn created_at(mut self, created_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.row_insert.created_at = created_at;
        self
    }

    pub fn author_id(mut self, author_id: Option<u64>) -> Self {
        self.row_insert.author_id = author_id;
        self
    }

    pub fn author_name(mut self, author_name: Option<String>) -> Self {
        self.row_insert.author_name = author_name;
        self
    }

    pub fn author_global_name(mut self, author_global_name: Option<String>) -> Self {
        self.row_insert.author_global_name = author_global_name;
        self
    }

    #[allow(dead_code)]
    pub fn play_count(mut self, play_count: i64) -> Self {
        self.row_insert.play_count = play_count;
        self
    }

    #[allow(dead_code)]
    pub fn last_played_at(mut self, last_played_at: Option<chrono::DateTime<chrono::Utc>>) -> Self {
        self.row_insert.last_played_at = last_played_at;
        self
    }

    #[allow(dead_code)]
    pub fn popularity(mut self, popularity: f64) -> Self {
        self.row_insert.popularity = popularity;
        self
    }

    #[allow(dead_code)]
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.row_insert.pinned = pinned;
        self
    }

    pub fn build(self) -> AudioTableRowInsert {
        self.row_insert
    }
}

impl AsRef<AudioTableRowInsert> for AudioTableRowInsert {
    fn as_ref(&self) -> &AudioTableRowInsert {
        &self
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub enum UniqueAudioTableCol {
    Id(i64),
    Name(String),
    AudioFile(String),
}

impl UniqueAudioTableCol {
    pub fn value(&self) -> String {
        match &self {
            Self::Id(val) => val.to_string(),
            Self::Name(val) => val.into(),
            Self::AudioFile(val) => val.into(),
        }
    }
}

impl AsRef<UniqueAudioTableCol> for UniqueAudioTableCol {
    fn as_ref(&self) -> &UniqueAudioTableCol {
        &self
    }
}

impl UniqueAudioTableCol {
    pub fn sql_condition(&self) -> String {
        match self {
            Self::Id(_) => format!("id = ? "),
            Self::Name(_) => format!("name = ? "),
            Self::AudioFile(_) => format!("audio_file = ? "),
        }
    }
}

pub struct AudioTable {
    conn: DbConnection,
}

impl AudioTable {
    pub const TABLE_NAME: &'static str = "audio";
    pub const FTS5_TABLE_NAME: &'static str = "fts5_audio";

    pub fn new(connection: DbConnection) -> Self {
        Self { conn: connection }
    }

    /// Return list of audio tracks by name that are most similiar to partial string
    /// **note**: If few than 3 chars entered, list of latest sounds added are returned
    pub fn fts_autocomplete_track_names(
        &self,
        partial: impl AsRef<str>,
        limit: Option<usize>,
    ) -> Vec<String> {
        let text = partial.as_ref();

        let limit = limit.unwrap_or(5);

        // low char query
        if text.len() < 3 {
            log::debug!("low character auto complete: '{text}'");
            let table_name = Self::TABLE_NAME;
            let sql =
                format!("SELECT name FROM {table_name} ORDER BY created_at DESC LIMIT {limit}");
            let mut stmt = self
                .conn
                .prepare(sql.as_str())
                .expect("Autocomplete low-char sql invalid");

            let rows = stmt.query_map((), |row| row.get("name"));
            match rows {
                Ok(rows) => {
                    let rows: Vec<String> = rows.filter_map(|row| row.ok()).collect();
                    return rows;
                }
                Err(err) => {
                    log::error!("Autocomplete low-char sql query error - {err}");
                    return vec![];
                }
            }
        }

        log::debug!("Auto complete partial search on {text}");
        let fts5_table_name = Self::FTS5_TABLE_NAME;
        let sql = format!("SELECT name FROM {fts5_table_name}(?) LIMIT {limit}");
        let mut stmt = self
            .conn
            .prepare(sql.as_str())
            .expect("Autocomplete sql invalid");

        let rows = stmt.query_map(params![&text], |row| row.get("name"));
        match rows {
            Ok(rows) => rows.filter_map(|row| row.ok()).collect(),
            Err(err) => {
                log::error!("Autocomplete sql query error - {err}");
                vec![]
            }
        }
    }

    pub fn find_audio_row(&self, col: impl AsRef<UniqueAudioTableCol>) -> Option<AudioTableRow> {
        let col = col.as_ref();
        let col_value = col.value();
        let table_name = Self::TABLE_NAME;

        let sql_condition = col.sql_condition();
        let sql = format!("SELECT * FROM {table_name} WHERE {sql_condition}");

        self.conn
            .query_row(sql.as_str(), params![&col_value], |row| {
                AudioTableRow::try_from(row)
            })
            .log_err_msg(format!("Failed to find audio row - {col:?}"))
            .ok()
    }

    pub fn insert_audio_row(
        &self,
        audio_row: impl AsRef<AudioTableRowInsert>,
    ) -> Result<(), String> {
        let audio_row = audio_row.as_ref();

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

        self.connection()
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

    pub fn update_audio_row(&self, audio_row: impl AsRef<AudioTableRow>) -> Result<(), String> {
        let audio_row = audio_row.as_ref();
        log::info!("Updating audio row. Name: {}", audio_row.name);

        let table_name = Self::TABLE_NAME;
        let name = &audio_row.name;
        let tags = &audio_row.tags;
        let row_id = audio_row.id;

        let sql = format!(
            "
            UPDATE {table_name}
            SET
                name = ?,
                tags = ?
            WHERE
                id = ?;
        "
        );

        self.conn
            .execute(sql.as_str(), params![&name, &tags, &row_id])
            .log_err_msg("Failed updating audio track")
            .map_err(|err| err.to_string())?;

        log::info!("Updated audio row. Name: {name}");
        Ok(())
    }

    pub fn increment_play_count(&self, row_id: i64) -> Result<(), String> {
        log::info!("Incrementing play count for audio row with id: {row_id}");

        let table_name = Self::TABLE_NAME;
        let last_played_at = chrono::Utc::now();

        let sql = format!(
            "UPDATE {table_name}
            SET
                play_count = play_count + 1,
                last_played_at = ?
            WHERE id = ?"
        );

        self.conn
            .execute(sql.as_str(), params![&last_played_at, &row_id])
            .log_err_msg("Failed incrementing play count")
            .map_err(|err| err.to_string())?;

        Ok(())
    }

    pub fn update_audio_row_pin_by_name(
        &self,
        audio_name: impl AsRef<str>,
        pinned: bool,
    ) -> Result<(), PoiseError> {
        let name = audio_name.as_ref();
        log::info!("Updating audio row pin by name: {name}, pinned: {pinned}");

        let table_name = Self::TABLE_NAME;
        let sql = format!("UPDATE {table_name} SET pinned = ? WHERE name = ?;");

        self.conn
            .execute(sql.as_str(), params![&pinned, &name])
            .log_err_msg("Failed to update audio row pin by name")
            .map_err(|err| PoiseError::from(err))?;

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

    pub fn get_random_row(&self) -> Result<Option<AudioTableRow>, String> {
        log::info!("Getting random audio row");

        let table_name = Self::TABLE_NAME;
        let sql = format!("SELECT * FROM {table_name} ORDER BY RANDOM() LIMIT 1");

        let result = self
            .conn
            .query_one(sql.as_str(), [], |row| AudioTableRow::try_from(row));

        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Table for AudioTable {
    fn connection(&self) -> &DbConnection {
        &self.conn
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
                    tags VARCHAR(2048),
                    audio_file VARCHAR(500) NOT NULL UNIQUE,
                    created_at VARCHAR(25) NOT NULL,
                    author_id INTEGER,
                    author_name VARCHAR(256),
                    author_global_name VARCHAR(256),
                    play_count INTEGER DEFAULT 0,
                    last_played_at VARCHAR(25) DEFAULT NULL,
                    popularity REAL DEFAULT 0,
                    pinned BOOLEAN DEFAULT FALSE
                );

                CREATE VIRTUAL TABLE IF NOT EXISTS {fts5_table_name} USING FTS5(
                    name, tags, content={table_name}, content_rowid=id, tokenize='trigram remove_diacritics 1'
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

#[allow(unused)]
#[derive(Debug)]
pub enum AudioTableOrderBy {
    CreatedAt(Order),
    Id(Order),
    Name(Order),
    PlayCount(Order),
}

impl AudioTableOrderBy {
    pub fn to_sql_str(&self) -> String {
        match self {
            Self::CreatedAt(order) => format!("created_at {order}"),
            Self::Id(order) => format!("id {order}"),
            Self::Name(order) => format!("name {order}"),
            Self::PlayCount(order) => format!("play_count {order}"),
        }
    }

    pub fn inverse_order(&self) -> Self {
        match self {
            Self::CreatedAt(order) => Self::CreatedAt(order.inverse()),
            Self::Id(order) => Self::Id(order.inverse()),
            Self::Name(order) => Self::Name(order.inverse()),
            Self::PlayCount(order) => Self::PlayCount(order.inverse()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::helpers::{self, uuid_v4_str};
    use audio::AudioFile;
    use r2d2_sqlite::SqliteConnectionManager;

    use super::*;

    fn get_db_connection() -> DbConnection {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::new(db_manager).unwrap();
        db_pool.get().unwrap()
    }

    fn get_audio_table() -> AudioTable {
        AudioTable::new(get_db_connection())
    }

    fn make_audio_table_row_insert() -> AudioTableRowInsert {
        let name = format!("{}{}", uuid_v4_str(), "#!@#$%^&*()_-+=?/.\"\\'");
        let audio_file = AudioFile::new(
            std::path::Path::new(&format!("/tmp/{}.mp3", helpers::uuid_v4_str())).to_path_buf(),
        );

        AudioTableRowInsertBuilder::new(name, audio_file)
            .tags(uuid_v4_str())
            .build()
    }

    #[test]
    fn table_create_test() {
        let table = get_audio_table();
        table.create_table(); // create table(s) & trigger(s)
        table.create_table(); // ignore table(s) & triggers(s) already created
    }

    #[test]
    fn table_insert_row_test() {
        let table = get_audio_table();

        table.create_table();
        table
            .insert_audio_row(make_audio_table_row_insert())
            .unwrap();
    }

    #[test]
    fn table_find_row_test() {
        let table = get_audio_table();
        table.create_table();

        let row_insert = make_audio_table_row_insert();
        table.insert_audio_row(&row_insert).unwrap();

        let row = table.find_audio_row(UniqueAudioTableCol::Name(row_insert.name.clone()));
        let row = row.unwrap();
        assert_eq!(row.name, row_insert.name);
    }

    #[test]
    fn table_update_row_test() {
        let table = get_audio_table();
        table.create_table();

        let row_insert = make_audio_table_row_insert();
        table.insert_audio_row(&row_insert).unwrap();

        let mut row = table
            .find_audio_row(UniqueAudioTableCol::Name(row_insert.name.clone()))
            .unwrap();

        let new_name = String::from("New Name");
        row.name = new_name.clone();
        table.update_audio_row(&row).unwrap();

        let old_row = table.find_audio_row(UniqueAudioTableCol::Name(row_insert.name.clone()));
        assert!(old_row.is_none());

        let updated_row = table
            .find_audio_row(UniqueAudioTableCol::Name(new_name.clone()))
            .unwrap();

        assert_eq!(updated_row.name, new_name);
    }

    #[test]
    fn table_autocomplete_track_names_test() {
        let table = get_audio_table();
        table.create_table();

        let mut row_insert = make_audio_table_row_insert();
        row_insert.name = "Beep Boop".into();
        row_insert.tags = Tags::from("r2d2 star wars droid");
        table.insert_audio_row(row_insert).unwrap();

        let mut row_insert = make_audio_table_row_insert();
        row_insert.name = "Beep Bop".into();
        row_insert.tags = Tags::from("gonk star wars droid");
        table.insert_audio_row(row_insert).unwrap();

        let mut row_insert = make_audio_table_row_insert();
        row_insert.name = "Beez's Biz".into();
        row_insert.tags = Tags::from("random sound-effect");
        table.insert_audio_row(row_insert).unwrap();

        let results = table.fts_autocomplete_track_names("bee", None);
        assert_eq!(3, results.len());

        let results = table.fts_autocomplete_track_names("bee", Some(2));
        assert_eq!(2, results.len());

        let results = table.fts_autocomplete_track_names("r2d2", None);
        assert_eq!("Beep Boop", results[0]);

        let results = table.fts_autocomplete_track_names("droid", None);
        assert_eq!(2, results.len());
        assert_eq!("Beep Boop", results[0]);
        assert_eq!("Beep Bop", results[1]);

        let results = table.fts_autocomplete_track_names("RaN", None);
        assert_eq!("Beez's Biz", results[0]);
    }

    #[test]
    fn tags_test() {
        let tags = Tags::from("tag-1, tag_2, tag3, !#$%^&tag4&*(()\ttag5");

        assert_eq!(
            &vec!["tag-1", "tag_2", "tag3", "tag4", "tag5"],
            tags.inner()
        );
    }
}
