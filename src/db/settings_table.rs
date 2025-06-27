use rusqlite::{params, OptionalExtension};

use crate::{commands::PoiseError, common::LogResult};

use super::{DbConnection, Table};

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
    conn: DbConnection,
}

impl SettingsTable {
    const TABLE_NAME: &'static str = "settings";

    pub fn new(connection: DbConnection) -> Self {
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
        let join_audio = settings.join_audio.as_ref();
        let leave_audio = settings.leave_audio.as_ref();

        let sql = format!(
            "
            UPDATE {table_name}
            SET
                join_audio = ?,
                leave_audio = ?
            WHERE
                id = ?;
            "
        );

        self.conn
            .execute(sql.as_str(), params![&join_audio, &leave_audio, &row_id])
            .log_err()?;

        Ok(())
    }
}

impl Table for SettingsTable {
    fn connection(&self) -> &DbConnection {
        &self.conn
    }

    fn drop_table(&self) {
        let table_name = Self::TABLE_NAME;
        log::info!("Dropping table: {table_name}");
        let sql = format!(
            "
            DROP TABLE IF EXISTS {table_name} (
                id INTEGER PRIMARY KEY,
                join_audio VARCHAR(80),
                leave_audio VARCHAR(80)
            );
        "
        );

        self.conn
            .execute_batch(sql.as_str())
            .log_err_msg("Failed dropping table")
            .log_ok_msg(format!("Dropped table {table_name}"))
            .unwrap();
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
}

#[cfg(test)]
mod tests {

    use r2d2_sqlite::SqliteConnectionManager;

    use super::*;

    fn get_settings_table() -> SettingsTable {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::new(db_manager).unwrap();
        let connection = db_pool.get().unwrap();
        SettingsTable::new(connection)
    }

    #[test]
    fn table_create_test() {
        let table = get_settings_table();
        table.create_table();
        table.create_table();
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

        let join_audio = Some(String::from("do!@)#$*&%&)'\"op"));
        let leave_audio = Some(String::from("dope"));

        settings.join_audio = join_audio.clone();
        settings.leave_audio = leave_audio.clone();

        table.update_settings(&settings).unwrap();

        let settings = table.get_settings().unwrap();

        assert_eq!(settings.join_audio, join_audio);
        assert_eq!(settings.leave_audio, leave_audio);
    }
}
