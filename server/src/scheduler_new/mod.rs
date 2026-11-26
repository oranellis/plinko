//! The tenant as one network: vertices are day boundaries and arcs are what takes days between
//! them. Everything is indexed densely and days are offsets from the table origin.

pub mod arc;
pub mod bits;
pub mod bound;
pub mod day;
pub mod justify;
pub mod ledger;
pub mod lower;
pub mod network;
pub mod objective;
pub mod pass;
pub mod search;
pub mod table;
