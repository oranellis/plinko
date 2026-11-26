//! The tenant as one network: vertices are day boundaries and arcs are what takes days between
//! them. Everything is indexed densely and days are offsets from the table origin.

pub mod arc;
pub mod bits;
pub mod day;
pub mod ledger;
pub mod lower;
pub mod network;
pub mod table;
