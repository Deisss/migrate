use mysql::*;
use mysql::prelude::*;
use super::{SqlEngine, EngineError};
use std::error::Error;
use std::path::PathBuf;
use std::result::Result;

pub struct Mysql {
    client: PooledConn,
    migration_table_name: String,
}

impl Mysql {
    /// Create MySQL
    pub fn new(url: &str, migration_table_name: &str) -> Result<Box<dyn SqlEngine>, Box<dyn Error>> {
        match Pool::new(url) {
            Ok(client) => {
                match client.get_conn() {
                    Ok(connection) => {
                        Ok(Box::new(Mysql {
                            client: connection,
                            migration_table_name: migration_table_name.to_owned(),
                        }))
                    },
                    Err(e) => {
                        crit!("Could not connect to MySQL: {}", e);
                        return Err(Box::new(e));
                    }
                }
            },
            Err(e) => {
                crit!("Could not create instance of MySQL: {}", e);
                Err(Box::new(e))
            }
        }
    }
}

impl SqlEngine for Mysql {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>> {
        let create_table = format!("CREATE TABLE IF NOT EXISTS `{}` (`migration` VARCHAR(20) PRIMARY KEY, `hash` VARCHAR(32), `type` VARCHAR(255), `file_name` TEXT, `created_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP)", self.migration_table_name);
        match self.client.query_drop(&create_table as &str) {
            Ok(_) => Ok(0),
            Err(e) => Err(Box::new(e))
        }
    }

    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let get_migration = format!("SELECT `migration` FROM `{}` ORDER BY `migration` DESC", self.migration_table_name);
        let data = self.client.query_map(&get_migration, |migration: String| {
            String::from(migration)
        });
        match data {
            Ok(data) => Ok(data),
            Err(e) => {
                crit!("Error getting migration: {}", e);
                Err(Box::new(e))
            }
        }
    }

    fn get_migrations_with_hashes(&mut self, migration_type: &str) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
        let get_migration = format!("SELECT `migration`, `hash`, `file_name` FROM `{}` WHERE `type` = ? ORDER BY `migration` DESC", self.migration_table_name);
        let data = self.client.exec_map(&get_migration, (&migration_type,), |(migration, hash, file_name): (String, String, String)| {
            (migration, hash, file_name)
        });
        match data {
            Ok(data) => Ok(data),
            Err(e) => {
                crit!("Error getting migration: {}", e);
                Err(Box::new(e))
            }
        }
    }

    fn migrate(&mut self, file: &PathBuf, version: &str, migration_type: &str, migration: &str, skip_transaction: bool) -> Result<(), Box<dyn Error>> {
        // Insert statement
        let insert = format!("INSERT INTO `{}` (`migration`, `hash`, `type`, `file_name`, `created_at`) VALUES (?, ?, ?, ?, NOW());", self.migration_table_name);
        match skip_transaction {
            true => {
                // Executing migration
                match self.client.query_drop(migration) {
                    Ok(_) => {
                        let hash = format!("{:x}", md5::compute(&migration));
                        let file_name = format!("{}", &file.display());

                        // Store in migration table and commit
                        match self.client.exec_drop(&insert as &str, (&version, &hash, &migration_type, &file_name,)) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                crit!("Could store result in migration table: {}", e.to_string());
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        crit!("{}", e);
                        Err(Box::new(EngineError {}))
                    }
                }
            },
            false => {
                // Do the transaction
                match self.client.start_transaction(TxOpts::default()) {
                    Ok(mut trx) => {
                        match trx.query_drop(migration) {
                            Ok(_) => {
                                let hash = format!("{:x}", md5::compute(&migration));
                                let file_name = format!("{}", &file.display());

                                // Store in migration table and commit
                                match trx.exec_drop(&insert as &str, (&version, &hash, &migration_type, &file_name,)) {
                                    Ok(_) => {
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
                                crit!("{}", e);
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
        // Delete statement
        let del = format!("DELETE FROM `{}` WHERE migration` = ?;", self.migration_table_name);
        match skip_transaction {
            true => {
                // Executing migration
                match self.client.query_drop(migration) {
                    Ok(_) => {
                        // Store in migration table and commit
                        match self.client.exec_drop(&del as &str, (&version,)) {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                crit!("Could store result in migration table: {}", e.to_string());
                                Err(Box::new(e))
                            }
                        }
                    },
                    Err(e) => {
                        crit!("{}", e);
                        Err(Box::new(EngineError {}))
                    }
                }
            },
            false => {
                // Do the transaction
                match self.client.start_transaction(TxOpts::default()) {
                    Ok(mut trx) => {
                        match trx.query_drop(migration) {
                            Ok(_) => {
                                // Store in migration table and commit
                                match trx.exec_drop(&del as &str, (&version,)) {
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
                                        crit!("Could store result in migration table: {}", e.to_string());
                                        Err(Box::new(e))
                                    }
                                }
                            },
                            Err(e) => {
                                crit!("{}", e);
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