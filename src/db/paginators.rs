use super::{
    audio_table::{AudioTableOrderBy, AudioTableRow},
    AudioTable, DbConnection,
};

#[derive(Debug)]
pub struct AudioTablePaginator {
    conn: DbConnection,
    order_by: AudioTableOrderBy,
    page_limit: u64,
    offset: u64,
}

impl AudioTablePaginator {
    pub fn builder(conn: DbConnection) -> AudioTablePaginatorBuilder {
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
    conn: DbConnection,
    order_by: AudioTableOrderBy,
    page_limit: u64,
}

impl AudioTablePaginatorBuilder {
    pub fn new(conn: DbConnection) -> Self {
        Self {
            conn: conn,
            order_by: AudioTableOrderBy::Id,
            page_limit: 500,
        }
    }

    #[allow(unused)]
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
    use r2d2_sqlite::SqliteConnectionManager;

    use crate::{
        audio::AudioFile,
        db::{audio_table::AudioTableRowInsert, Table},
        helpers::{self, uuid_v4_str},
    };

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
        AudioTableRowInsert {
            audio_file: AudioFile::new(
                std::path::Path::new(&format!("/tmp/{}.mp3", helpers::uuid_v4_str())).to_path_buf(),
            ),
            author_global_name: None,
            name: format!("{}{}", uuid_v4_str(), "#!@#$%^&*()_-+=?/.\"\\'"),
            tags: uuid_v4_str().into(),
            created_at: chrono::Utc::now(),
            author_id: None,
            author_name: None,
        }
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
}
