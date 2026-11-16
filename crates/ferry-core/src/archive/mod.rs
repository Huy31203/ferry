//! The shelf's "Add to archive" (shelf archive spec): zip only.

mod entries;

pub use entries::{Entry, collect, default_name, free_path, taken};
