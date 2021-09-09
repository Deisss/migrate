use postgres::{Client, Config, NoTls};
use std::str::FromStr;
use super::{SqlEngine, EngineError};
use std::error::Error;
use crate::helpers::get_relevant_line;
use std::path::PathBuf;
use md5;
use native_tls::TlsConnector;
use postgres_native_tls::MakeTlsConnector;

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
        str_error = (&str_error[1..len]).to_owned();
    }

    // Move from postgres Error to DBError
    let source = error.into_source();
    let source: Option<&(dyn std::error::Error + 'static)> = source.as_ref().map(|e| &**e as _);

    let downcast = match source.and_then(|e| e.downcast_ref::<postgres::error::DbError>()) {
        Some(d) => d,
        None => {
            crit!("");
            crit!("SQL Error: {}", str_error);
            crit!("");
            return;
        }
    };

    // Extract position from that error
    let position = downcast.position();
    if position.is_none() {
        crit!("");
        crit!("SQL Error: {}: {}", downcast.code().code(), str_error);
        crit!("");
        return;
    }
    let position = format!("{:?}", downcast.position().unwrap())
        .replace("Original(", "").replace(")", "");
    let position = position.parse::<u32>();
    if position.is_err() {
        crit!("");
        crit!("SQL Error: {}: {}", downcast.code().code(), str_error);
        crit!("");
        return;
    }
    let position = position.unwrap();
    let result = get_relevant_line(content, position);
    if result.is_none() {
        crit!("");
        crit!("SQL Error: {}: {}", downcast.code().code(), str_error);
        crit!("");
        return;
    }
    let result = result.unwrap();
    let trimmed = result.2.trim();
    let spaces: u32 = position - result.0 - 1;
    let spaces_trimmed: usize = spaces as usize - (result.2.len() - trimmed.len());

    // Printing the error
    crit!("");
    crit!("{} line {} column {}:", downcast.severity(), result.1, spaces);
    crit!("");
    crit!("{}", trimmed);
    let debug = format!("{}^ {}: {}", std::iter::repeat(" ").take(spaces_trimmed).collect::<String>(),
                         downcast.code().code(),
                         downcast.message());
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
        let config = match Config::from_str(url) {
            Ok(c) => c,
            Err(e) => {
                crit!("Could not create configuration for PostgreSQL: {}", e);
                return Err(Box::new(e));
            }

        };

        // We start by trying to connect with NoTls activated
        // If it fails we try then to connect with TLS...
        match config.connect(NoTls) {
            Ok(connection) => {
                Ok(Box::new(Postgresql {
                    client: connection,
                    migration_table_name: migration_table_name.to_owned(),
                }))
            },
            Err(_e) => {
                match TlsConnector::new() {
                    Ok(connector) => {
                        let connector = MakeTlsConnector::new(connector);
                        match config.connect(connector) {
                            Ok(connection) => {
                                Ok(Box::new(Postgresql {
                                    client: connection,
                                    migration_table_name: migration_table_name.to_owned(),
                                }))
                            },
                            Err(e) => {
                                if e.to_string().starts_with("error parsing response from server") {
                                    crit!("Could not connect to PostgreSQL: check credentials");
                                } else {
                                    crit!("Could not connect to PostgreSQL: {}", e);
                                }
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        crit!("Could not get TLS for PostgreSQL: {}", e);
                        Err(Box::new(e))
                    }
                }
            }
        }
    }
}

impl SqlEngine for Postgresql {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>> {
        let mut create_table: String = String::from("CREATE TABLE IF NOT EXISTS \"");
        create_table.push_str(&self.migration_table_name);
        create_table.push_str("\" (\"migration\" TEXT PRIMARY KEY, \"hash\" TEXT, \"type\" TEXT, \"file_name\" TEXT, \"created_at\" TIMESTAMP)");
        match self.client.execute(&create_table as &str, &[]) {
            Ok(i) => Ok(i),
            Err(e) => Err(Box::new(e))
        }
    }

    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let mut get_migration = String::from("SELECT \"migration\" FROM \"");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("\" ORDER BY \"migration\" desc");

        match self.client.query(&get_migration as &str, &[]) {
            Ok(results) => Ok(results.iter().map(|row| row.get(0)).collect::<Vec<String>>()),
            Err(e) => {
                crit!("Error getting migration: {}", e);
                Err(Box::new(e))
            }
        }

    }

    fn get_migrations_with_hashes(&mut self, migration_type: &str) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
        let mut get_migration = String::from("SELECT \"migration\", \"hash\", \"file_name\" FROM \"");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("\" WHERE \"type\" = $1 ORDER BY \"migration\" desc");
        match self.client.query(&get_migration as &str, &[&migration_type]) {
            Ok(results) => Ok(results.iter().map(|row| (row.get(0), row.get(1), row.get(2))).collect::<Vec<(String, String, String)>>()),
            Err(e) => {
                crit!("Error getting migration: {}", e);
                Err(Box::new(e))
            }
        }
    }

    fn migrate(&mut self, file: &PathBuf, version: &str, migration_type: &str, migration: &str, skip_transaction: bool) -> Result<(), Box<dyn Error>> {
        // Insert statement
        let mut insert = String::from("INSERT INTO \"");
        insert.push_str(&self.migration_table_name);
        insert.push_str("\" (\"migration\", \"hash\", \"type\", \"file_name\", \"created_at\") VALUES ($1, $2, $3, $4, NOW());");

        if skip_transaction {
            // Inserting migration
            match self.client.batch_execute(migration) {
                Ok(_) => {},
                Err(e) => {
                    print_error_postgres(migration, e);
                    return Err(Box::new(EngineError {}));
                }
            };

            let hash = format!("{:x}", md5::compute(&migration));
            let file_name = format!("{}", &file.display());

            // Store in migration table and commit
            match self.client.query(&insert as &str, &[&version, &hash, &migration_type, &file_name]) {
                Ok(_) => Ok(()),
                Err(e) => {
                    crit!("Could store result in migration table: {}", e.to_string());
                    return Err(Box::new(e));
                }
            }

        } else {
            // Do the transaction
            let trx = self.client.transaction();
            if trx.is_err() {
                let err = trx.err().unwrap();
                crit!("Could not create a transaction: {}", err.to_string());
                return Err(Box::new(err));
            }

            // Executing migration
            let mut trx = trx.unwrap();
            match trx.batch_execute(migration) {
                Ok(_) => {},
                Err(e) => {
                    print_error_postgres(migration, e);
                    return Err(Box::new(EngineError {}));
                }
            };

            let hash = format!("{:x}", md5::compute(&migration));
            let file_name = format!("{}", &file.display());

            // Store in migration table and commit
            match trx.query(&insert as &str, &[&version, &hash, &migration_type, &file_name]) {
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

    fn rollback(&mut self, _file: &PathBuf, version: &str, migration: &str, skip_transaction: bool) -> Result<(), Box<dyn Error>> {
        // Delete statement
        let mut del = String::from("DELETE FROM \"");
        del.push_str(&self.migration_table_name);
        del.push_str("\" WHERE \"migration\" = $1;");

        match skip_transaction {
            true => {
                // Inserting migration
                match self.client.batch_execute(migration) {
                    Ok(_) => {
                        // Store in migration table and commit
                        match self.client.query(&del as &str, &[&version]) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                crit!("Could store result in migration table: {}", e.to_string());
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        print_error_postgres(migration, e);
                        Err(Box::new(EngineError {}))
                    }
                }
            },
            false => {
                let mut trx = match self.client.transaction() {
                    Ok(t) => t,
                    Err(e) => {
                        crit!("Could not create a transaction: {}", e);
                        return Err(Box::new(e));
                    }
                };

                match trx.batch_execute(migration) {
                    Ok(_) => {},
                    Err(e) => {
                        print_error_postgres(migration, e);
                        return Err(Box::new(EngineError {}));
                    }
                };

                // Store in migration table and commit
                match trx.query(&del as &str, &[&version]) {
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
    }
}
