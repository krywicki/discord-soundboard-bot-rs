use crate::db;

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
    fts_filter: Option<String>,
    pinned: Option<bool>,
    limit: Option<u64>, // Limit for the total number of rows to fetch
}

pub struct PaginateInfo {
    pub first_page_offset: Option<u64>,
    pub prev_page_offset: Option<u64>,
    pub next_page_offset: Option<u64>,
    pub last_page_offset: Option<u64>,
    pub total_pages: u64,
    pub cur_page: u64,
    #[allow(unused)]
    pub total_row_count: u64,
    #[allow(unused)]
    pub page_limit: u64,
}

impl AudioTablePaginator {
    pub fn pageinate_info(&self) -> Result<PaginateInfo, String> {
        let row_count = self.row_count()?;

        let total_pages = row_count / self.page_limit;
        let cur_page = if row_count > 0 {
            (self.offset / self.page_limit) + 1
        } else {
            0
        };

        let first_page_offset = if cur_page == 1 || row_count == 0 {
            None
        } else {
            Some(0)
        };

        let last_page_offset = if cur_page == total_pages || row_count == 0 {
            None
        } else {
            Some((total_pages - 1) * self.page_limit)
        };

        let prev_page_offset = if (self.offset as i64 - self.page_limit as i64) < 0 {
            None
        } else {
            Some(self.offset - self.page_limit)
        };

        let next_page_offset = if (self.offset + self.page_limit) >= row_count {
            None
        } else {
            Some(self.offset + self.page_limit)
        };

        Ok(PaginateInfo {
            first_page_offset: first_page_offset,
            prev_page_offset: prev_page_offset,
            next_page_offset: next_page_offset,
            last_page_offset: last_page_offset,
            total_pages: total_pages,
            cur_page: cur_page,
            total_row_count: row_count,
            page_limit: self.page_limit,
        })
    }

    pub fn row_count(&self) -> Result<u64, String> {
        let conn = &self.conn;
        let audio_table_name = AudioTable::TABLE_NAME;
        let fts_table_name = AudioTable::FTS5_TABLE_NAME;
        let fts_filter = if let Some(fts_filter) = self.fts_filter.as_ref() {
            Some(self.fts_escape(fts_filter))
        } else {
            None
        };
        let mut where_sql: Vec<String> = vec![];
        let mut params: Vec<(&'static str, &dyn rusqlite::ToSql)> = vec![];

        let limit_sql = if let Some(limit) = self.limit {
            format!("LIMIT {limit}")
        } else {
            String::new()
        };

        if let Some(pinned) = self.pinned.as_ref() {
            where_sql.push("pinned = :pinned".into());
            params.push((":pinned", pinned));
        }

        let where_sql = if where_sql.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_sql.join(" AND "))
        };

        let sql = match fts_filter.as_ref() {
            Some(fts_filter) => {
                params.insert(0, (":fts_filter", fts_filter));

                // fts filtering
                format!(
                    "SELECT Audio.id FROM {audio_table_name} Audio
                    INNER JOIN {fts_table_name}(:fts_filter) FTS
                        ON Audio.id = FTS.rowid
                    {where_sql}
                    {limit_sql}
                    "
                )
            }
            None => {
                format!(
                    "SELECT id FROM {audio_table_name}
                    {where_sql}
                    {limit_sql}
                    "
                )
            }
        };

        let sql = format!("SELECT COUNT(id) FROM ({sql});");

        let mut stmt = conn
            .prepare(sql.as_ref())
            .expect("Failed to prepare sql stmt");

        let count: u64 = stmt
            .query_row(params.as_slice(), |row| row.get(0))
            .map_err(|err| format!("Error counting in AudioTablePaginator - {err}"))?;

        Ok(count)
    }

    fn fts_escape(&self, fts: impl AsRef<str>) -> String {
        let fts = fts.as_ref();

        format!("\"{}\"", fts.replace('"', "\"\""))
    }

    pub fn next_page(&mut self) -> Result<Vec<AudioTableRow>, String> {
        let conn = &self.conn;
        let audio_table_name = AudioTable::TABLE_NAME;
        let fts_table_name = AudioTable::FTS5_TABLE_NAME;
        let order_by_sql = self.order_by.to_sql_str();
        let offset = self.offset;
        let fts_filter = if let Some(fts_filter) = self.fts_filter.as_ref() {
            Some(self.fts_escape(fts_filter))
        } else {
            None
        };

        let mut where_sql: Vec<String> = vec![];
        let mut params: Vec<(&'static str, &dyn rusqlite::ToSql)> = vec![];
        let mut page_limit = self.page_limit;

        if let Some(limit) = self.limit {
            // If the page limit exceeds the total limit, adjust it
            if page_limit > limit {
                page_limit = limit;
                log::warn!(
                    "AudioTable Paginator Page limit ({page_limit}) exceeds total limit ({limit}) and has been adjusted."
                );
            }

            if self.offset >= limit {
                return Ok(vec![]);
            } else if self.offset + page_limit > limit {
                // Adjust the page limit if it exceeds the total limit
                page_limit = limit - self.offset;
            }
        }

        if let Some(pinned) = self.pinned.as_ref() {
            where_sql.push("pinned = :pinned".into());
            params.push((":pinned", pinned));
        }

        let where_sql = if where_sql.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_sql.join(" AND "))
        };

        let sql = match fts_filter.as_ref() {
            Some(fts_filter) => {
                params.insert(0, (":fts_filter", fts_filter));

                // fts filtering
                format!(
                    "SELECT Audio.* FROM {audio_table_name} Audio
                    INNER JOIN {fts_table_name}(:fts_filter) FTS
                        ON Audio.id = FTS.rowid
                    {where_sql}
                    ORDER BY {order_by_sql}
                    LIMIT {page_limit}
                    OFFSET {offset};
                    "
                )
            }
            None => {
                format!(
                    "SELECT * FROM {audio_table_name}
                    {where_sql}
                    ORDER BY {order_by_sql}
                    LIMIT {page_limit}
                    OFFSET {offset};
                    "
                )
            }
        };

        let mut stmt = conn
            .prepare(sql.as_ref())
            .expect("Failed to prepare sql stmt");

        let row_iter = stmt
            .query_map(params.as_slice(), |row| AudioTableRow::try_from(row))
            .map_err(|err| format!("Error in AudioTablePaginator - {err}"))?;

        let rows: Vec<AudioTableRow> = row_iter
            .filter_map(|row| match row {
                Ok(val) => Some(val),
                Err(err) => {
                    log::error!("{err}");
                    None
                }
            })
            .collect();

        self.offset += rows.len() as u64;

        Ok(rows)
    }
}

pub struct AudioTablePaginatorBuilder {
    paginator: AudioTablePaginator,
}

impl AudioTablePaginatorBuilder {
    pub fn new(conn: DbConnection) -> Self {
        Self {
            paginator: AudioTablePaginator {
                conn: conn,
                order_by: AudioTableOrderBy::Id(db::Order::Asc),
                page_limit: 500,
                fts_filter: None,
                pinned: None,
                offset: 0,
                limit: None,
            },
        }
    }

    pub fn most_recently_added_template(conn: DbConnection) -> Self {
        Self::new(conn)
            .order_by(AudioTableOrderBy::CreatedAt(db::Order::Desc))
            .page_limit(20)
    }

    pub fn most_played_template(conn: DbConnection) -> Self {
        Self::new(conn)
            .order_by(AudioTableOrderBy::PlayCount(db::Order::Desc))
            .page_limit(20)
    }

    pub fn search_template(conn: DbConnection, fts_filter: impl AsRef<str>) -> Self {
        let fts_filter = fts_filter.as_ref();
        Self::new(conn)
            .fts_filter(Some(fts_filter.into()))
            .page_limit(20)
    }

    pub fn all_template(conn: DbConnection) -> Self {
        Self::new(conn).page_limit(20)
    }

    pub fn pinned_template(conn: DbConnection) -> Self {
        Self::new(conn)
            .pinned(Some(true))
            .order_by(AudioTableOrderBy::Name(db::Order::Asc))
    }

    #[allow(unused)]
    pub fn order_by(mut self, value: AudioTableOrderBy) -> Self {
        self.paginator.order_by = value;
        self
    }

    pub fn page_limit(mut self, value: u64) -> Self {
        self.paginator.page_limit = value;
        self
    }

    pub fn fts_filter(mut self, value: Option<String>) -> Self {
        self.paginator.fts_filter = value;
        self
    }

    pub fn pinned(mut self, value: Option<bool>) -> Self {
        self.paginator.pinned = value;
        self
    }

    #[allow(unused)]
    pub fn limit(mut self, value: Option<u64>) -> Self {
        self.paginator.limit = value;
        self
    }

    pub fn offset(mut self, value: u64) -> Self {
        self.paginator.offset = value;
        self
    }

    pub fn build(self) -> AudioTablePaginator {
        self.paginator
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
        db::{
            audio_table::{AudioTableRowInsert, AudioTableRowInsertBuilder},
            Table,
        },
        helpers::{self, uuid_v4_str},
    };

    use super::*;

    fn make_audio_table_row_insert() -> AudioTableRowInsert {
        let name = format!("{}{}", uuid_v4_str(), "#!@#$%^&*()_-+=?/.\"\\'");
        let audio_file = AudioFile::new(
            std::path::Path::new(&format!("/tmp/{}.mp3", helpers::uuid_v4_str())).to_path_buf(),
        );

        AudioTableRowInsertBuilder::new(name, audio_file)
            .tags(uuid_v4_str())
            .build()
    }

    fn make_detailed_audio_table_row_insert(name: &str, tags: &str) -> AudioTableRowInsert {
        let mut table_row = make_audio_table_row_insert();
        table_row.name = name.into();
        table_row.tags = tags.into();
        table_row
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

        assert_eq!(paginator.row_count().unwrap(), 3);

        let page = paginator.next().unwrap().unwrap();
        assert_eq!(page.len(), 2);

        let page = paginator.next().unwrap().unwrap();
        assert_eq!(page.len(), 1);

        let page = paginator.next();
        assert!(page.is_none());
    }

    #[test]
    fn audio_table_pagination_limit_test() {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::new(db_manager).unwrap();
        let table = AudioTable::new(db_pool.get().unwrap());
        table.create_table();

        for _ in 0..3 {
            table
                .insert_audio_row(make_audio_table_row_insert())
                .unwrap();
        }

        // Test pagination with limit
        {
            let mut paginator = AudioTablePaginator::builder(db_pool.get().unwrap())
                .page_limit(1)
                .limit(Some(2))
                .build();

            assert_eq!(paginator.row_count().unwrap(), 2);

            let page = paginator.next().unwrap().unwrap();
            assert_eq!(page.len(), 1);

            let page = paginator.next().unwrap().unwrap();
            assert_eq!(page.len(), 1);

            let page = paginator.next();
            assert!(page.is_none());
        }

        // Test pagination page_limit exceeds total limit
        {
            let mut paginator = AudioTablePaginator::builder(db_pool.get().unwrap())
                .page_limit(5)
                .limit(Some(3))
                .build();

            assert_eq!(paginator.row_count().unwrap(), 3);

            let page = paginator.next().unwrap().unwrap();
            assert_eq!(page.len(), 3);

            let page = paginator.next();
            assert!(page.is_none());
        }
    }

    #[test]
    fn audio_table_fts_pagination_test() {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::new(db_manager).unwrap();
        let table = AudioTable::new(db_pool.get().unwrap());
        table.create_table();

        table
            .insert_audio_row(make_detailed_audio_table_row_insert(
                "star wars obi wan",
                "",
            ))
            .unwrap();
        table
            .insert_audio_row(make_detailed_audio_table_row_insert(
                "han solo",
                "star wars",
            ))
            .unwrap();
        table
            .insert_audio_row(make_detailed_audio_table_row_insert(
                "i'll be back",
                "terminator two",
            ))
            .unwrap();

        // plain fts filter
        {
            let mut paginator = AudioTablePaginator::builder(db_pool.get().unwrap())
                .page_limit(2)
                .fts_filter(Some("star".into()))
                .build();

            assert_eq!(paginator.row_count().unwrap(), 2);

            let page = paginator.next().unwrap().unwrap();
            assert_eq!(page.len(), 2);
            assert_eq!(page[0].name, "star wars obi wan");
            assert_eq!(page[1].name, "han solo");

            let page = paginator.next();
            assert!(page.is_none());
        }

        // fts edge case
        {
            let mut paginator = AudioTablePaginator::builder(db_pool.get().unwrap())
                .fts_filter(Some("asdfasdfasdfasdf".into()))
                .build();

            assert_eq!(paginator.row_count().unwrap(), 0);

            let page = paginator.next();
            assert!(page.is_none());

            paginator = AudioTablePaginator::builder(db_pool.get().unwrap())
                .fts_filter(Some("@''\"''\"@#$%^&*()!".into()))
                .build();

            assert_eq!(paginator.row_count().unwrap(), 0);

            let page = paginator.next();
            assert!(page.is_none());
        }
    }

    #[test]
    fn audio_table_offset_test() {
        let db_manager = SqliteConnectionManager::memory();
        let db_pool = r2d2::Pool::new(db_manager).unwrap();
        let table = AudioTable::new(db_pool.get().unwrap());
        table.create_table();

        let mut row = make_audio_table_row_insert();
        row.name = "first".into();
        row.tags = "tag1".into();
        table.insert_audio_row(row).unwrap();

        row = make_audio_table_row_insert();
        row.name = "second".into();
        row.tags = "tag2".into();
        table.insert_audio_row(row).unwrap();

        row = make_audio_table_row_insert();
        row.name = "third".into();
        row.tags = "tag1".into();
        table.insert_audio_row(row).unwrap();

        row = make_audio_table_row_insert();
        row.name = "fourth".into();
        row.tags = "tag2".into();
        table.insert_audio_row(row).unwrap();

        row = make_audio_table_row_insert();
        row.name = "fifth".into();
        row.tags = "tag1".into();
        table.insert_audio_row(row).unwrap();

        let mut paginator = AudioTablePaginator::builder(db_pool.get().unwrap())
            .fts_filter(Some("tag1".into()))
            .page_limit(1)
            .offset(2)
            .build();

        assert_eq!(paginator.row_count().unwrap(), 3);

        let page = paginator.next().unwrap().unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].name, "fifth");

        let page = paginator.next();
        assert!(page.is_none());
    }
}
