pub mod audio_table;
pub mod paginators;
pub mod settings_table;

use core::fmt;

pub use audio_table::{AudioTable, AudioTableRow, Tags, UniqueAudioTableCol};
pub use paginators::AudioTablePaginator;
pub use settings_table::SettingsTable;

pub type DbConnection = r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>;

pub trait Table {
    fn connection(&self) -> &DbConnection;
    fn create_table(&self);
}

#[derive(Debug)]
pub enum Order {
    Asc,
    Desc,
}

impl From<Order> for String {
    fn from(order: Order) -> Self {
        order.into()
    }
}

impl From<&Order> for String {
    fn from(order: &Order) -> Self {
        match order {
            Order::Asc => "ASC".to_string(),
            Order::Desc => "DESC".to_string(),
        }
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from(self))
    }
}
