use std::env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub homeserver_url: String,
    pub bot_user: String,
    pub bot_password: String,
    pub data_dir: String,
    pub cerebras_api_key: String,
    pub cerebras_model: String,
    pub cerebras_base_url: String,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let homeserver_url = required_var("HOMESERVER_URL");
        let bot_user = required_var("BOT_USER");
        let bot_password = required_var("BOT_PASSWORD");
        let cerebras_api_key = required_var("CEREBRAS_API_KEY");
        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_owned());
        let cerebras_model =
            env::var("CEREBRAS_MODEL").unwrap_or_else(|_| "llama3.1-8b".to_owned());
        let cerebras_base_url = env::var("CEREBRAS_BASE_URL")
            .unwrap_or_else(|_| "https://api.cerebras.ai/v1".to_owned());
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| format!("sqlite://{data_dir}/fern.db"));

        Self {
            homeserver_url,
            bot_user,
            bot_password,
            data_dir,
            cerebras_api_key,
            cerebras_model,
            cerebras_base_url,
            database_url,
        }
    }
}

fn required_var(name: &str) -> String {
    match env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!("Missing required environment variable: {name}"),
    }
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

        let vars = [
            "HOMESERVER_URL",
            "BOT_USER",
            "BOT_PASSWORD",
            "DATA_DIR",
            "CEREBRAS_API_KEY",
            "CEREBRAS_MODEL",
            "CEREBRAS_BASE_URL",
            "DATABASE_URL",
        ];
        let originals: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|name| ((*name).to_owned(), std::env::var(name).ok()))
            .collect();

        std::env::set_var("HOMESERVER_URL", "http://localhost:6167");
        std::env::set_var("BOT_USER", "@fern:local");
        std::env::set_var("BOT_PASSWORD", "");
        std::env::set_var("CEREBRAS_API_KEY", "test-key");
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

    #[test]
    fn missing_cerebras_api_key_panics_with_clear_message() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");

        let vars = [
            "HOMESERVER_URL",
            "BOT_USER",
            "BOT_PASSWORD",
            "DATA_DIR",
            "CEREBRAS_API_KEY",
            "CEREBRAS_MODEL",
            "CEREBRAS_BASE_URL",
            "DATABASE_URL",
        ];
        let originals: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|name| ((*name).to_owned(), std::env::var(name).ok()))
            .collect();

        std::env::set_var("HOMESERVER_URL", "http://localhost:6167");
        std::env::set_var("BOT_USER", "@fern:local");
        std::env::set_var("BOT_PASSWORD", "secret");
        std::env::set_var("CEREBRAS_API_KEY", "");
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
            message.contains("Missing required environment variable: CEREBRAS_API_KEY"),
            "unexpected panic message: {message}"
        );
    }
}
