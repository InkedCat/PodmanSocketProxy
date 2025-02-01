use serde::Deserialize;
use std::fs;
use thiserror::Error;

#[derive(Error, Debug)]
pub struct HTTPRegexParseError {
    method: String,
    #[source]
    source: regex::Error,
}

impl HTTPRegexParseError {
    pub fn new(method: String, source: regex::Error) -> Self {
        HTTPRegexParseError { method, source }
    }
}

impl std::fmt::Display for HTTPRegexParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[{}] {}", self.method, self.source)
    }
}

#[derive(Error, Debug)]
pub enum ConfigParsingError {
    #[error("failed to parse TOML file")]
    ParseError(#[from] toml::de::Error),
    #[error("failed to read TOML file")]
    ReadError(#[from] std::io::Error),
    #[error(transparent)]
    RegexError(#[from] HTTPRegexParseError),
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub get: Proxy,
    pub head: Proxy,
    pub post: Proxy,
    pub put: Proxy,
    pub patch: Proxy,
    pub delete: Proxy,
}

#[derive(Deserialize, Clone)]
pub struct Proxy {
    pub allowed: bool,
    pub regex: String,
}

/// Check if the regex strings in the config are valid
///
/// # Errors
///
/// If an invalid pattern is found in the config, an error is returned.
/// An error is also returned if the pattern is valid, but would
/// produce a regex that is bigger than the size limit configured in the regex library.
fn check_config_regex(config: &Config) -> Result<(), HTTPRegexParseError> {
    let methods = vec!["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"];
    for method in methods {
        let proxy = match method {
            "GET" => &config.get,
            "HEAD" => &config.head,
            "POST" => &config.post,
            "PUT" => &config.put,
            "PATCH" => &config.patch,
            "DELETE" => &config.delete,
            _ => return Ok(()),
        };

        if proxy.allowed {
            match regex::Regex::new(&proxy.regex) {
                Ok(_) => {}
                Err(e) => {
                    return Err(HTTPRegexParseError::new(method.to_string(), e));
                }
            };
        }
    }

    Ok(())
}

pub fn get_config(path: &str) -> Result<Config, ConfigParsingError> {
    let config_file = fs::read_to_string(path)?;
    let config = toml::from_str(&config_file)?;

    check_config_regex(&config)?;

    Ok(config)
}
