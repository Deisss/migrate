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
/// * `content` - The SQL content having problem.
/// * `error` - The error found.
/// * `logger` - Logger instance.
fn print_error_postgres(content: &str, error: postgres::error::Error, logger: &slog::Logger) {
    let mut str_error = format!("{}", error);

    if str_error.starts_with("\"") && str_error.ends_with("\"") {
        let len = str_error.len() - 1;
        str_error = (&str_error[1..len]).to_owned();
    }

    // Move from postgres Error to DBError
    let source = error.into_source();
    let source: Option<&(dyn std::error::Error + 'static)> = source.as_ref().map(|e| &**e as _);

    match source.and_then(|e| e.downcast_ref::<postgres::error::DbError>()) {
        Some(downcast) => {
            match downcast.position() {
                Some(position) => {
                    let position = format!("{:?}", position).replace("Original(", "").replace(")", "");
                    match position.parse::<u32>() {
                        Ok(position) => {
                            match get_relevant_line(content, position) {
                                Some(result) => {
                                    let trimmed = result.2.trim();
                                    let spaces: u32 = position - result.0 - 1;
                                    let spaces_trimmed: usize = spaces as usize - (result.2.len() - trimmed.len());

                                    // Printing the error
                                    slog::crit!(logger, "");
                                    slog::crit!(logger, "{} line {} column {}:", downcast.severity(), result.1, spaces);
                                    slog::crit!(logger, "");
                                    slog::crit!(logger, "{}", trimmed);
                                    let debug = format!("{}^ {}: {}", std::iter::repeat(" ").take(spaces_trimmed).collect::<String>(),
                                                         downcast.code().code(),
                                                         downcast.message());
                                    slog::crit!(logger, "{}", debug);
                                    slog::crit!(logger, "");
                                },
                                None => {
                                    slog::crit!(logger, "");
                                    slog::crit!(logger, "SQL Error: {}: {}", downcast.code().code(), str_error);
                                    slog::crit!(logger, "");
                                }
                            };
                        },
                        Err(_e) => {
                            slog::crit!(logger, "");
                            slog::crit!(logger, "SQL Error: {}: {}", downcast.code().code(), str_error);
                            slog::crit!(logger, "");
                        }
                    };
                },
                None => {
                    slog::crit!(logger, "");
                    slog::crit!(logger, "SQL Error: {}: {}", downcast.code().code(), str_error);
                    slog::crit!(logger, "");
                }
            };
        },
        None => {
            slog::crit!(logger, "");
            slog::crit!(logger, "SQL Error: {}", str_error);
            slog::crit!(logger, "");
        }
    };
}


pub struct Postgresql {
    client: Client,
    migration_table_name: String,
    logger: slog::Logger,
}

impl Postgresql {
    /// Create PostgreSQL
    pub fn new(url: &str, migration_table_name: &str, logger: &slog::Logger) -> Result<Box<dyn SqlEngine>, Box<dyn Error>> {
        let config = match Config::from_str(url) {
            Ok(c) => c,
            Err(e) => {
                slog::crit!(logger, "Could not create configuration for PostgreSQL: {}", e);
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
                    logger: logger.clone(),
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
                                    logger: logger.clone(),
                                }))
                            },
                            Err(e) => {
                                if e.to_string().starts_with("error parsing response from server") {
                                    slog::crit!(logger, "Could not connect to PostgreSQL: check credentials");
                                } else {
                                    slog::crit!(logger, "Could not connect to PostgreSQL: {}", e);
                                }
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        slog::crit!(logger, "Could not get TLS for PostgreSQL: {}", e);
                        Err(Box::new(e))
                    }
                }
            }
        }
    }
}

impl SqlEngine for Postgresql {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>> {
        let create_table = format!("CREATE TABLE IF NOT EXISTS \"{}\" (\"migration\" TEXT PRIMARY KEY, \"hash\" TEXT, \"type\" TEXT, \"file_name\" TEXT, \"created_at\" TIMESTAMP)", self.migration_table_name);
        match self.client.execute(&create_table as &str, &[]) {
            Ok(i) => Ok(i),
            Err(e) => Err(Box::new(e))
        }
    }

    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let get_migration = format!("SELECT \"migration\" FROM \"{}\" ORDER BY \"migration\" DESC", self.migration_table_name);
        match self.client.query(&get_migration as &str, &[]) {
            Ok(results) => Ok(results.iter().map(|row| row.get(0)).collect::<Vec<String>>()),
            Err(e) => {
                slog::crit!(&self.logger, "Error getting migration: {}", e);
                Err(Box::new(e))
            }
        }

    }

    fn get_migrations_with_hashes(&mut self, migration_type: &str) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
        let get_migration = format!("SELECT \"migration\", \"hash\", \"file_name\" FROM \"{}\" WHERE \"type\" = $1 ORDER BY \"migration\" DESC", self.migration_table_name);
        match self.client.query(&get_migration as &str, &[&migration_type]) {
            Ok(results) => Ok(results.iter().map(|row| (row.get(0), row.get(1), row.get(2))).collect::<Vec<(String, String, String)>>()),
            Err(e) => {
                slog::crit!(&self.logger, "Error getting migration: {}", e);
                Err(Box::new(e))
            }
        }
    }

    fn migrate(&mut self, file: &PathBuf, version: &str, migration_type: &str, migration: &str, skip_transaction: bool) -> Result<(), Box<dyn Error>> {
        let insert = format!("INSERT INTO \"{}\" (\"migration\", \"hash\", \"type\", \"file_name\", \"created_at\") VALUES ($1, $2, $3, $4, NOW());", self.migration_table_name);
        match skip_transaction {
            true => {
                // Inserting migration
                match self.client.batch_execute(migration) {
                    Ok(_) => {
                        let hash = format!("{:x}", md5::compute(&migration));
                        let file_name = format!("{}", &file.display());

                        // Store in migration table and commit
                        match self.client.query(&insert as &str, &[&version, &hash, &migration_type, &file_name]) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                slog::crit!(&self.logger, "Could store result in migration table: {}", e);
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        print_error_postgres(migration, e, &self.logger);
                        Err(Box::new(EngineError {}))
                    }
                }
            },
            false => {
                // Do the transaction
                match self.client.transaction() {
                    Ok(mut trx) => {
                        // Executing migration
                        match trx.batch_execute(migration) {
                            Ok(_) => {
                                let hash = format!("{:x}", md5::compute(&migration));
                                let file_name = format!("{}", &file.display());

                                // Store in migration table and commit
                                match trx.query(&insert as &str, &[&version, &hash, &migration_type, &file_name]) {
                                    Ok(_) => {
                                        // Committing results
                                        match trx.commit() {
                                            Ok(_) => Ok(()),
                                            Err(e) => {
                                                slog::crit!(&self.logger, "Failed to commit transaction: {}", e);
                                                Err(Box::new(e))
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        slog::crit!(&self.logger, "Could store result in migration table: {}", e);
                                        Err(Box::new(e))
                                    }
                                }
                            },
                            Err(e) => {
                                print_error_postgres(migration, e, &self.logger);
                                Err(Box::new(EngineError {}))
                            }
                        }
                    },
                    Err(e) => {
                        slog::crit!(&self.logger, "Could not create a transaction: {}", e);
                        Err(Box::new(e))
                    }
                }
            }
        }
    }

    fn rollback(&mut self, _file: &PathBuf, version: &str, migration: &str, skip_transaction: bool) -> Result<(), Box<dyn Error>> {
        let del = format!("DELETE FROM \"{}\" WHERE \"migration\" = $1;", self.migration_table_name);
        match skip_transaction {
            true => {
                // Inserting migration
                match self.client.batch_execute(migration) {
                    Ok(_) => {
                        // Store in migration table and commit
                        match self.client.query(&del as &str, &[&version]) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                slog::crit!(&self.logger, "Could store result in migration table: {}", e);
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        print_error_postgres(migration, e, &self.logger);
                        Err(Box::new(EngineError {}))
                    }
                }
            },
            false => {
                match self.client.transaction() {
                    Ok(mut trx) => {
                        // Execute SQL
                        match trx.batch_execute(migration) {
                            Ok(_) => {
                                // Store in migration table and commit
                                match trx.query(&del as &str, &[&version]) {
                                    Ok(_) => {
                                        // Committing result
                                        match trx.commit() {
                                            Ok(_) => Ok(()),
                                            Err(e) => {
                                                slog::crit!(&self.logger, "Failed to commit transaction: {}", e);
                                                Err(Box::new(e))
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        slog::crit!(&self.logger, "Could store result in migration table: {}", e);
                                        Err(Box::new(e))
                                    }
                                }
                            },
                            Err(e) => {
                                print_error_postgres(migration, e, &self.logger);
                                Err(Box::new(EngineError {}))
                            }
                        }
                    },
                    Err(e) => {
                        slog::crit!(&self.logger, "Could not create a transaction: {}", e);
                        Err(Box::new(e))
                    }
                }


            }
        }
    }
}
