pub mod audio_table;
pub mod paginators;
pub mod settings_table;

pub use audio_table::{AudioTable, AudioTableRow, AudioTableRowInsert, Tags, UniqueAudioTableCol};
pub use paginators::AudioTablePaginator;
pub use settings_table::SettingsTable;

pub type DbConnection = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

pub trait Table {
    fn connection(&self) -> &DbConnection;
    fn create_table(&self);
}
