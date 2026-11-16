//! The shelf's "Add to archive" (shelf archive spec): zip only.

mod entries;
mod write;

pub use entries::{Entry, collect, default_name, free_path, taken};
pub use write::{Encryption, Options, parse_size, write};
