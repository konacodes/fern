use std::env;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub signal_api_url: String,
    pub signal_account_number: String,
    pub data_dir: String,
    pub cerebras_api_key: String,
    pub cerebras_model: String,
    pub cerebras_base_url: String,
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub database_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();

        let signal_api_url = required_var("SIGNAL_API_URL");
        let signal_account_number = required_var("SIGNAL_ACCOUNT_NUMBER");
        let cerebras_api_key = required_var("CEREBRAS_API_KEY");
        let data_dir = env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_owned());
        let cerebras_model =
            env::var("CEREBRAS_MODEL").unwrap_or_else(|_| "llama3.1-8b".to_owned());
        let cerebras_base_url = env::var("CEREBRAS_BASE_URL")
            .unwrap_or_else(|_| "https://api.cerebras.ai/v1".to_owned());
        let anthropic_api_key = optional_var("ANTHROPIC_API_KEY");
        let anthropic_model =
            env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".to_owned());
        let database_url =
            env::var("DATABASE_URL").unwrap_or_else(|_| format!("sqlite://{data_dir}/fern.db"));

        Self {
            signal_api_url,
            signal_account_number,
            data_dir,
            cerebras_api_key,
            cerebras_model,
            cerebras_base_url,
            anthropic_api_key,
            anthropic_model,
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

fn optional_var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
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
            "SIGNAL_API_URL",
            "SIGNAL_ACCOUNT_NUMBER",
            "DATA_DIR",
            "CEREBRAS_API_KEY",
            "CEREBRAS_MODEL",
            "CEREBRAS_BASE_URL",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_MODEL",
            "DATABASE_URL",
        ];
        let originals: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|name| ((*name).to_owned(), std::env::var(name).ok()))
            .collect();

        std::env::set_var("SIGNAL_API_URL", "");
        std::env::set_var("SIGNAL_ACCOUNT_NUMBER", "+15550000000");
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
            message.contains("Missing required environment variable: SIGNAL_API_URL"),
            "unexpected panic message: {message}"
        );
    }

    #[test]
    fn missing_cerebras_api_key_panics_with_clear_message() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");

        let vars = [
            "SIGNAL_API_URL",
            "SIGNAL_ACCOUNT_NUMBER",
            "DATA_DIR",
            "CEREBRAS_API_KEY",
            "CEREBRAS_MODEL",
            "CEREBRAS_BASE_URL",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_MODEL",
            "DATABASE_URL",
        ];
        let originals: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|name| ((*name).to_owned(), std::env::var(name).ok()))
            .collect();

        std::env::set_var("SIGNAL_API_URL", "http://signal-api:8080");
        std::env::set_var("SIGNAL_ACCOUNT_NUMBER", "+15550000000");
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

    #[test]
    fn config_loads_signal_fields() {
        let _guard = env_lock().lock().expect("env lock should not be poisoned");

        let vars = [
            "SIGNAL_API_URL",
            "SIGNAL_ACCOUNT_NUMBER",
            "DATA_DIR",
            "CEREBRAS_API_KEY",
            "CEREBRAS_MODEL",
            "CEREBRAS_BASE_URL",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_MODEL",
            "DATABASE_URL",
        ];
        let originals: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|name| ((*name).to_owned(), std::env::var(name).ok()))
            .collect();

        std::env::set_var("SIGNAL_API_URL", "http://signal-api:8080");
        std::env::set_var("SIGNAL_ACCOUNT_NUMBER", "+15550000000");
        std::env::set_var("CEREBRAS_API_KEY", "test-key");
        std::env::set_var("DATA_DIR", "./data");

        let config = Config::from_env();
        assert_eq!(config.signal_api_url, "http://signal-api:8080");
        assert_eq!(config.signal_account_number, "+15550000000");

        for (name, value) in originals {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }
}
