use native_tls::TlsConnector;
use postgres::{Client, Config};
use postgres_native_tls::{MakeTlsConnector};
use std::str::FromStr;
use super::{SqlEngine, EngineError};
use std::error::Error;
use crate::helpers::get_relevant_line;
use std::path::PathBuf;
use md5;

/// Print on console the PostgreSQL error.
///
/// # Arguments
///
/// * `file` - The SQL file having problem.
/// * `content` - The SQL content having problem.
/// * `error` - The error found.
fn print_error_postgres(content: &str, error: postgres::error::Error) {
    let mut str_error = format!("{}", error);

    if str_error.starts_with("\"") && str_error.ends_with("\"") {
        let len = str_error.len() - 1;
        str_error = (&str_error[1..len]).to_string();
    }

    // Move from postgres Error to DBError
    let source = error.into_source();
    let source: Option<&(dyn std::error::Error + 'static)> = source.as_ref().map(|e| &**e as _);
    let downcasted = source.and_then(|e| e.downcast_ref::<postgres::error::DbError>());
    if downcasted.is_none() {
        crit!("");
        crit!("SQL Error: {}", str_error);
        crit!("");
        return;
    }
    let downcasted = downcasted.unwrap();

    // Extract position from that error
    let position = downcasted.position();
    if position.is_none() {
        crit!("");
        crit!("SQL Error: {}: {}", downcasted.code().code(), str_error);
        crit!("");
        return;
    }
    let position = format!("{:?}", downcasted.position().unwrap())
        .replace("Original(", "").replace(")", "");
    let position = position.parse::<u32>();
    if position.is_err() {
        crit!("");
        crit!("SQL Error: {}: {}", downcasted.code().code(), str_error);
        crit!("");
        return;
    }
    let position = position.unwrap();
    let result = get_relevant_line(content, position);
    if result.is_none() {
        crit!("");
        crit!("SQL Error: {}: {}", downcasted.code().code(), str_error);
        crit!("");
        return;
    }
    let result = result.unwrap();
    let trimmed = result.2.trim();
    let spaces: u32 = position - result.0 - 1;
    let spaces_trimmed: usize = spaces as usize - (result.2.len() - trimmed.len());

    // Printing the error
    crit!("");
    crit!("{} line {} column {}:", downcasted.severity(), result.1, spaces);
    crit!("");
    crit!("{}", trimmed);
    let debug = format!("{}^ {}: {}", std::iter::repeat(" ").take(spaces_trimmed).collect::<String>(),
                         downcasted.code().code(),
                         downcasted.message());
    crit!("{}", debug);
    crit!("");
}

pub struct Postgresql {
    client: Client,
    migration_table_name: String,
}

impl Postgresql {
    /// Create PostgreSQL
    pub fn new(url: &str, migration_table_name: &str) -> Result<Box<dyn SqlEngine>, Box<dyn Error>> {
        let config = Config::from_str(url);
        if config.is_err() {
            let err = config.err().unwrap();
            crit!("Could not create configuration for PostgreSQL: {}", err.to_string());
            return Err(Box::new(err));
        }
        let config = config.unwrap();
        let connector = TlsConnector::new();
        if connector.is_err() {
            let err = connector.err().unwrap();
            crit!("Could not get TLS for PostgreSQL: {}", err.to_string());
            return Err(Box::new(err));
        }
        let connector = MakeTlsConnector::new(connector.unwrap());
        let connection = config.connect(connector);
        if connection.is_err() {
            let err = connection.err().unwrap();
            if err.to_string().starts_with("error parsing response from server") {
                crit!("Could not connect to PostgreSQL: check credentials");
            } else {
                crit!("Could not connect to PostgreSQL: {}", err.to_string());
            }
            return Err(Box::new(err));
        }
        let connection = connection.unwrap();
        Ok(Box::new(Postgresql {
            client: connection,
            migration_table_name: migration_table_name.to_owned(),
        }))
    }
}

impl SqlEngine for Postgresql {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>> {
        let mut create_table: String = String::from("CREATE TABLE IF NOT EXISTS \"");
        create_table.push_str(&self.migration_table_name);
        create_table.push_str("\" (\"migration\" TEXT PRIMARY KEY, \"hash\" TEXT, \"file_name\" TEXT, \"created_at\" TIMESTAMP)");
        match self.client.execute(&create_table as &str, &[]) {
            Ok(i) => Ok(i),
            Err(e) => Err(Box::new(e))
        }
    }

    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let mut results: Vec<String> = Vec::new();
        let mut get_migration = String::from("SELECT \"migration\" FROM \"");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("\" ORDER BY \"migration\" desc");
        let data = self.client.query(&get_migration as &str, &[]);
        if data.is_err() {
            let err = data.err().unwrap();
            crit!("Error getting migration: {}", err.to_string());
            return Err(Box::new(err));
        }
        for row in data.unwrap() {
            results.push(row.get(0));
        }
        Ok(results)
    }

    fn get_migrations_with_hashes(&mut self) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
        let mut results: Vec<(String, String, String)> = Vec::new();
        let mut get_migration = String::from("SELECT \"migration\", \"hash\", \"file_name\" FROM \"");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("\" ORDER BY \"migration\" desc");
        let data = self.client.query(&get_migration as &str, &[]);
        if data.is_err() {
            let err = data.err().unwrap();
            crit!("Error getting migration: {}", err.to_string());
            return Err(Box::new(err));
        }
        for row in data.unwrap() {
            results.push((row.get(0), row.get(1), row.get(2)));
        }
        Ok(results)
    }

    fn migrate(&mut self, file: &PathBuf, version: &str, migration: &str) -> Result<(), Box<dyn Error>> {
        // Insert statement
        let mut insert = String::from("INSERT INTO \"");
        insert.push_str(&self.migration_table_name);
        insert.push_str("\" (\"migration\", \"hash\", \"file_name\", \"created_at\") VALUES ($1, $2, $3, NOW());");

        // Do the transaction
        let trx = self.client.transaction();
        if trx.is_err() {
            let err = trx.err().unwrap();
            crit!("Could not create a transaction: {}", err.to_string());
            return Err(Box::new(err));
        }

        // Executing migration
        let mut trx = trx.unwrap();
        match trx.execute(migration, &[]) {
            Ok(_) => {},
            Err(e) => {
                print_error_postgres(migration, e);
                return Err(Box::new(EngineError {}));
            }
        };

        let hash = format!("{:x}", md5::compute(&migration));
        let file_name = format!("{}", &file.display());

        // Store in migration table and commit
        match trx.execute(&insert as &str, &[&version, &hash, &file_name]) {
            Ok(_) => {},
            Err(e) => {
                crit!("Could store result in migration table: {}", e.to_string());
                return Err(Box::new(e));
            }
        };
        match trx.commit() {
            Ok(_) => Ok(()),
            Err(e) => {
                crit!("Failed to commit transaction: {}", e.to_string());
                Err(Box::new(e))
            }
        }
    }

    fn rollback(&mut self, _file: &PathBuf, version: &str, migration: &str) -> Result<(), Box<dyn Error>> {
        // Delete statement
        let mut del = String::from("DELETE FROM \"");
        del.push_str(&self.migration_table_name);
        del.push_str("\" WHERE \"migration\" = $1;");

        // Do the transaction
        let trx = self.client.transaction();
        if trx.is_err() {
            let err = trx.err().unwrap();
            crit!("Could not create a transaction: {}", err.to_string());
            return Err(Box::new(err));
        }

        // Executing migration
        let mut trx = trx.unwrap();
        match trx.execute(migration, &[]) {
            Ok(_) => {},
            Err(e) => {
                print_error_postgres(migration, e);
                return Err(Box::new(EngineError {}));
            }
        };

        // Store in migration table and commit
        match trx.execute(&del as &str, &[&version]) {
            Ok(_) => {},
            Err(e) => {
                crit!("Could store result in migration table: {}", e.to_string());
                return Err(Box::new(e));
            }
        };
        match trx.commit() {
            Ok(_) => Ok(()),
            Err(e) => {
                crit!("Failed to commit transaction: {}", e.to_string());
                Err(Box::new(e))
            }
        }
    }
}
