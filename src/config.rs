use std::env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub homeserver_url: String,
    pub bot_user: String,
    pub bot_password: String,
    pub data_dir: String,
}

impl Config {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let homeserver_url = required_var("HOMESERVER_URL");
        let bot_user = required_var("BOT_USER");
        let bot_password = required_var("BOT_PASSWORD");
        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_owned());

        Self {
            homeserver_url,
            bot_user,
            bot_password,
            data_dir,
        }
    }
}

fn required_var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("Missing required environment variable: {name}"))
}

#[cfg(test)]
mod tests {
    use std::panic;
    use std::sync::{Mutex, OnceLock};

    use super::Config;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn missing_env_var_panics_with_clear_message() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");

        let vars = ["HOMESERVER_URL", "BOT_USER", "BOT_PASSWORD", "DATA_DIR"];
        let originals: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|name| ((*name).to_owned(), std::env::var(name).ok()))
            .collect();

        std::env::set_var("HOMESERVER_URL", "http://localhost:6167");
        std::env::set_var("BOT_USER", "@fern:local");
        std::env::remove_var("BOT_PASSWORD");
        std::env::set_var("DATA_DIR", "./data");

        let panic_result = panic::catch_unwind(Config::from_env);

        for (name, value) in originals {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }

        let payload = panic_result.expect_err("Config::from_env should panic");
        let message = if let Some(message) = payload.downcast_ref::<String>() {
            message.clone()
        } else if let Some(message) = payload.downcast_ref::<&str>() {
            (*message).to_owned()
        } else {
            String::new()
        };

        assert!(
            message.contains("Missing required environment variable: BOT_PASSWORD"),
            "unexpected panic message: {message}"
        );
    }
}
