//! Screenshot, screen-recording and folder watching (stream C). Everything but
//! the thread in `service` is pure: a settle state machine with an injected
//! clock, a router from file to source, and a ledger of Ferry's own writes
//! (`selfwrite`) so Ferry never reacts to what it wrote itself.

pub mod capture;
pub mod engine;
pub mod events;
pub mod filter;
pub mod media;
pub mod plan;
pub mod probe;
pub mod settle;
