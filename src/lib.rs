pub mod bot;
pub mod config;

pub use bot::{format_echo, should_echo, EchoMessage, FernBot};
pub use config::Config;
