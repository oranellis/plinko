//! The tenant as one network: vertices are day boundaries and arcs are what takes days between
//! them. Everything is indexed densely and days are offsets from the table origin.

pub mod bits;
pub mod day;
pub mod table;
