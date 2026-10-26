//! Screenshot, screen-recording and folder watching (stream C). Everything but
//! the thread in `service` is pure: a settle state machine with an injected
//! clock, a router from file to source, and a ledger of Ferry's own writes
//! (`selfwrite`) so Ferry never reacts to what it wrote itself.

pub mod capture;
pub mod engine;
pub mod events;
pub mod filter;
#[cfg(test)]
mod live_tests;
pub mod media;
pub mod plan;
pub mod probe;
pub mod service;
pub mod settle;

pub static DIAG_T0: std::sync::LazyLock<std::time::Instant> =
    std::sync::LazyLock::new(std::time::Instant::now);

#[macro_export]
macro_rules! wdiag {
    ($($arg:tt)*) => {
        eprintln!("[wdiag {:>6}ms {:?}] {}", $crate::watch::DIAG_T0.elapsed().as_millis(), std::thread::current().name(), format!($($arg)*))
    };
}
