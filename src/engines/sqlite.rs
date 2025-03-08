use rusqlite::Connection;
use super::{SqlEngine, EngineError};
use std::error::Error;
use std::path::PathBuf;
use md5;

pub struct Sqlite {
    client: Connection,
    migration_table_name: String,
}

impl Sqlite {
    /// Create SQLite
    pub fn new(url: &str, migration_table_name: &str) -> Result<Box<dyn SqlEngine>, Box<dyn Error>> {
        match Connection::open(url) {
            Ok(connection) => {
                Ok(Box::new(Sqlite {
                    client: connection,
                    migration_table_name: migration_table_name.to_owned(),
                }))
            },
            Err(e) => {
                crit!("Could not open database for SqLite: {}", e);
                Err(Box::new(e))
            }
        }
    }
}

impl SqlEngine for Sqlite {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>> {
        let create_table = format!("CREATE TABLE IF NOT EXISTS \"{}\" (\"migration\" TEXT PRIMARY KEY, \"hash\" TEXT, \"type\" TEXT, \"file_name\" TEXT, \"created_at\" TIMESTAMP)", self.migration_table_name);
        match self.client.execute(&create_table as &str, []) {
            Ok(_) => Ok(0),
            Err(e) => Err(Box::new(e))
        }
    }

    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let get_migration = format!("SELECT \"migration\" FROM \"{}\" ORDER BY \"migration\" DESC", self.migration_table_name);
        let mut stmt = self.client.prepare(&get_migration as &str)?;
        let mut results: Vec<String> = Vec::new();
        let _ = stmt.query_map([], |row| {
            let tmp = row.get(0);
            if tmp.is_ok() {
                results.push(tmp.unwrap());
            }
            Ok(())
        })?;
        Ok(results)
    }

    fn get_migrations_with_hashes(&mut self, migration_type: &str) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
        let get_migration = format!("SELECT \"migration\", \"hash\", \"file_name\" FROM \"{}\" WHERE \"type\" = $1 ORDER BY \"migration\" DESC", self.migration_table_name);
        let mut stmt = self.client.prepare(&get_migration as &str)?;
        let mut results: Vec<(String, String, String)> = Vec::new();
        let _ = stmt.query_map(&[&migration_type], |row| {
            let migration_name = row.get(0);
            let migration_hash = row.get(1);
            let migration_file = row.get(2);
            if migration_name.is_ok() {
                results.push((migration_name.unwrap(), migration_hash.unwrap(), migration_file.unwrap()));
            }
            Ok(())
        })?;
        Ok(results)
    }

    fn migrate(&mut self, file: &PathBuf, version: &str, migration_type: &str, migration: &str, skip_transaction: bool) -> Result<(), Box<dyn Error>> {
        let insert = format!("INSERT INTO \"{}\" (\"migration\", \"hash\", \"type\", \"file_name\", \"created_at\") VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP);", self.migration_table_name);
        match skip_transaction {
            true => {
                // Do the transaction
                match self.client.execute(migration, []) {
                    Ok(_) => {
                        let hash = format!("{:x}", md5::compute(&migration));
                        let file_name = format!("{}", &file.display());

                        // Store in migration table and commit
                        match self.client.execute(&insert as &str, &[&version, &hash[..], &migration_type, &file_name]) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                crit!("Could store result in migration table: {}", e.to_string());
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        println!("{:?}", e);
                        Err(Box::new(EngineError {}))
                    }
                }
            },
            false => {
                // Starting transaction
                match self.client.transaction() {
                    Ok(trx) => {
                        // Doing SQL
                        match trx.execute(migration, []) {
                            Ok(_) => {
                                let hash = format!("{:x}", md5::compute(&migration));
                                let file_name = format!("{}", &file.display());

                                // Store in migration table and commit
                                match trx.execute(&insert as &str, &[&version, &hash[..], &migration_type, &file_name]) {
                                    Ok(_) => {
                                        // Committing transaction
                                        match trx.commit() {
                                            Ok(_) => Ok(()),
                                            Err(e) => {
                                                crit!("Failed to commit transaction: {}", e.to_string());
                                                Err(Box::new(e))
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        crit!("Could store result in migration table: {}", e);
                                        Err(Box::new(e))
                                    }
                                }
                            },
                            Err(e) => {
                                println!("{:?}", e);
                                Err(Box::new(EngineError {}))
                            }
                        }
                    },
                    Err(e) => {
                        crit!("Could not create a transaction: {}", e);
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
                // Do the transaction
                match self.client.execute(migration, []) {
                    Ok(_) => {
                        // Store in migration table
                        match self.client.execute(&del as &str, &[&version]) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                crit!("Could store result in migration table: {}", e.to_string());
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        println!("{:?}", e);
                        Err(Box::new(EngineError {}))
                    }
                }

            },
            false => {
                // Do the transaction
                match self.client.transaction() {
                    Ok(trx) => {
                        // Doing the migration
                        match trx.execute(migration, []) {
                            Ok(_) => {
                                // Store in migration table and commit
                                match trx.execute(&del as &str, &[&version]) {
                                    Ok(_) => {
                                        // Committing results
                                        match trx.commit() {
                                            Ok(_) => Ok(()),
                                            Err(e) => {
                                                crit!("Failed to commit transaction: {}", e.to_string());
                                                Err(Box::new(e))
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        crit!("Could store result in migration table: {}", e.to_string());
                                        Err(Box::new(e))
                                    }
                                }
                            },
                            Err(e) => {
                                println!("{:?}", e);
                                Err(Box::new(EngineError {}))
                            }
                        }
                    },
                    Err(e) => {
                        crit!("Could not create a transaction: {}", e);
                        Err(Box::new(e))
                    }
                }
            }
        }
    }
}
