//! Monday.com integration module.
//!
//! Provides configuration types for linking a plinko plan to a Monday.com board.

pub mod config;

pub use config::{
    BoardColumn, ColumnMap, ItemNodeMapping, MondayConfig, MondayItem, MondayUser, StatusMapping,
    UserMapping,
};
