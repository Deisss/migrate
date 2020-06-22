use crate::Configuration;
use crate::EngineName;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
mod postgresql;
mod sqlite;
mod mysql;

// Define our error types. These may be customized for our error handling cases.
// Now we will be able to write our own errors, defer to an underlying error
// implementation, or do something in between.
#[derive(Debug, Clone)]
pub struct EngineError;

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "error while doing migration")
    }
}

impl Error for EngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

pub trait SqlEngine {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>>;
    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>>;
    fn get_migrations_with_hashes(&mut self, migration_type: &str) -> Result<Vec<(String, String, String)>, Box<dyn Error>>;
    fn migrate(&mut self, file: &PathBuf, version: &str, migration_type: &str, migration: &str) -> Result<(), Box<dyn Error>>;
    fn rollback(&mut self, file: &PathBuf, version: &str, migration: &str) -> Result<(), Box<dyn Error>>;
}

/// Generate the URL for postgresql connexion.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
fn postgresql_url(configuration: &Configuration) -> String {
    if configuration.url.len() > 0 {
        return configuration.url.clone();
    }
    let mut url = String::from("host='");
    url.push_str(&configuration.host);
    url.push_str("' user='");
    url.push_str(&configuration.username);
    url.push('\'');

    if configuration.port != 5432 {
        url.push_str(" port=");
        url.push_str(&configuration.port.to_string());
    }

    if configuration.password.len() > 0 {
        url.push_str(" password='");
        url.push_str(&configuration.password);
        url.push('\'');
    }

    if configuration.database != "postgres" {
        url.push_str(" dbname='");
        url.push_str(&configuration.database);
        url.push('\'');
    }

    url
}

/// Generate the URL for sqlite connexion.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
fn sqlite_url(configuration: &Configuration) -> String {
    if configuration.url.len() > 0 {
        return configuration.url.clone();
    }
    String::from(&configuration.host)
}

/// Generate the URL for mysql connexion.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
fn mysql_url(configuration: &Configuration) -> String {
    if configuration.url.len() > 0 {
        return configuration.url.clone();
    }
    let mut url = String::from("mysql://");

    if configuration.username.len() > 0 {
        url.push_str(&configuration.username);
    } else {
        url.push_str("root");
    }

    if configuration.password.len() > 0 {
        url.push(':');
        url.push_str(&configuration.password);
    }

    url.push('@');
    url.push_str(&configuration.host);

    if configuration.port != 3306 {
        url.push(':');
        url.push_str(&configuration.port.to_string());
    }

    if configuration.database.len() > 0 {
        url.push('/');
        url.push_str(&configuration.database);
    }

    url
}

/// Factory for creating instance of the right SQL engine.
///
/// # Arguments
///
/// * `name` - The engine name (like mysql, postgres, ...).
/// * `configuration` - The configuration to use.
pub fn get_sql_engine(name: &EngineName, configuration: &Configuration) -> Result<Box<dyn SqlEngine>, Box<dyn Error>> {
    match name {
        EngineName::SQLITE => sqlite::Sqlite::new(&sqlite_url(configuration), &configuration.table),
        EngineName::POSTGRESQL => postgresql::Postgresql::new(&postgresql_url(configuration), &configuration.table),
        EngineName::MYSQL => mysql::Mysql::new(&mysql_url(configuration), &configuration.table),
    }
}
