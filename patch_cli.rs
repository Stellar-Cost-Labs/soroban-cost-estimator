use std::sync::atomic::{AtomicU8, Ordering};

pub static COLOR_CHOICE: AtomicU8 = AtomicU8::new(0);

pub fn init_color(choice: clap::ColorChoice) {
    let val = match choice {
        clap::ColorChoice::Auto => 0,
        clap::ColorChoice::Always => 1,
        clap::ColorChoice::Never => 2,
    };
    COLOR_CHOICE.store(val, Ordering::Relaxed);
}

pub fn should_colorize() -> bool {
    match COLOR_CHOICE.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            use std::io::IsTerminal;
            std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
        }
    }
}
