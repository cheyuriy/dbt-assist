use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Sets the verbose mode. When enabled, the `vprintln!` macro will print messages to the console. This is typically called once at the start of the program based on CLI arguments.
pub fn set_verbose(v: bool) {
    VERBOSE.store(v, Ordering::Relaxed);
}

/// Checks if verbose mode is enabled. This is used by the `vprintln!` macro to determine whether to print messages.
pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// A macro for printing verbose messages. Usage: `vprintln!("Value: {}", value);` Works like `println!` but only prints if verbose mode is enabled.
#[macro_export]
macro_rules! vprintln {
    ($($arg:tt)*) => {
        if $crate::verbose::is_verbose() {
            println!($($arg)*);
        }
    };
}
