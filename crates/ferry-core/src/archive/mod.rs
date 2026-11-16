//! The shelf's "Add to archive" (shelf archive spec): zip only.

mod entries;
mod rates;
mod write;

pub use entries::{Entry, collect, default_name, free_path, taken};
pub use rates::Rates;
pub use write::{Encryption, Options, parse_size, write};
