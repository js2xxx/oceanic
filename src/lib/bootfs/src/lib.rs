//! The bootfs.
//!
//!     |   Header   |
//!     |------------|
//!     |   Entries  |
//!     |   ...      |
//!     |------------|
//!     |  Dir/File  |
//!     |  Content   |
//!     |  ...       |

#![no_std]
#![feature(int_roundings)]

#[cfg(feature = "gen")]
pub mod gen;
pub mod parse;
mod types;

pub use self::types::*;

#[cfg(feature = "gen")]
extern crate std;
