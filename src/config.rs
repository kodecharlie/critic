use std::env;
use std::fs;
use std::path::PathBuf;
use ini::Ini;

#[derive(Debug, Clone)]
pub struct Config {
    pub apple_host: String,
    pub apple_key: String,
    pub jira_host: Option<String>,
    pub jira_apikey: Option<String>,
    pub db_host: String,
    pub db_user: String,
    pub db_pass: String,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let home = env::var("HOME").map_err(|_| "HOME environment variable not set")?;
        let config_path = PathBuf::from(home).join(".criticrc");
        
        let ini = Ini::load_from_file(&config_path)
            .unwrap_or_else(|_| Ini::new()); // Fallback or handle missing file

        let apple_host = ini.get_from(Some("apple"), "apple.api.host").unwrap_or("developer.apple.com").to_string();
        let apple_key = ini.get_from(Some("apple"), "apple.api.key").unwrap_or("").to_string();
        
        let jira_host = ini.get_from(Some("jira"), "jira.host").map(|s| s.to_string());
        let jira_apikey = ini.get_from(Some("jira"), "jira.apikey").map(|s| s.to_string());

        let db_host = ini.get_from(Some("embeddings"), "embeddings.host").unwrap_or("localhost").to_string();
        let db_user = ini.get_from(Some("embeddings"), "embeddings.user").unwrap_or("critic").to_string();
        let db_pass = ini.get_from(Some("embeddings"), "embeddings.password").unwrap_or("critic123").to_string();

        Ok(Config {
            apple_host,
            apple_key,
            jira_host,
            jira_apikey,
            db_host,
            db_user,
            db_pass,
        })
    }
}