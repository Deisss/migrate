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
        let client = Pool::new(url);
        if client.is_err() {
            let err = client.err().unwrap();
            crit!("Could not create instance of MySQL: {}", err.to_string());
            return Err(Box::new(err));
        }
        let client = client.unwrap();
        let connection = client.get_conn();
        if connection.is_err() {
            let err = connection.err().unwrap();
            crit!("Could not connect to MySQL: {}", err.to_string());
            return Err(Box::new(err));
        }
        Ok(Box::new(Mysql {
            client: connection.unwrap(),
            migration_table_name: migration_table_name.to_owned(),
        }))
    }
}

impl SqlEngine for Mysql {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>> {
        let mut create_table: String = String::from("CREATE TABLE IF NOT EXISTS `");
        create_table.push_str(&self.migration_table_name);
        create_table.push_str("` (`migration` VARCHAR(20) PRIMARY KEY, `hash` VARCHAR(32), `file_name` TEXT, `created_at` TIMESTAMP DEFAULT CURRENT_TIMESTAMP)");
        match self.client.query_drop(&create_table as &str) {
            Ok(_) => Ok(0),
            Err(e) => Err(Box::new(e))
        }
    }

    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        //let mut results: Vec<String> = Vec::new();
        let mut get_migration = String::from("SELECT `migration` FROM `");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("` ORDER BY `migration` desc");

        let data = self.client.query_map(&get_migration, |migration: String| {
            String::from(migration)
        });

        if data.is_err() {
            let err = data.err().unwrap();
            crit!("Error getting migration: {}", err.to_string());
            return Err(Box::new(err));
        }
        Ok(data.unwrap())
    }

    fn get_migrations_with_hashes(&mut self) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
        let mut get_migration = String::from("SELECT `migration`, `hash`, `file_name` FROM `");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("` ORDER BY `migration` desc");

        let data = self.client.query_map(&get_migration, |(migration, hash, file_name): (String, String, String)| {
            (migration, hash, file_name)
        });

        if data.is_err() {
            let err = data.err().unwrap();
            crit!("Error getting migration: {}", err.to_string());
            return Err(Box::new(err));
        }
        Ok(data.unwrap())
    }

    fn migrate(&mut self, file: &PathBuf, version: &str, migration: &str) -> Result<(), Box<dyn Error>> {
        // Insert statement
        let mut insert = String::from("INSERT INTO `");
        insert.push_str(&self.migration_table_name);
        insert.push_str("` (`migration`, `hash`, `file_name`, `created_at`) VALUES (?, ?, ?, NOW());");

        // Do the transaction
        let trx = self.client.start_transaction(TxOpts::default());
        if trx.is_err() {
            let err = trx.err().unwrap();
            crit!("Could not create a transaction: {}", err.to_string());
            return Err(Box::new(err));
        }

        // Executing migration
        let mut trx = trx.unwrap();
        match trx.query_drop(migration) {
            Ok(_) => {},
            Err(e) => {
                crit!("{}", e);
                //print_error_postgres(migration, e);
                return Err(Box::new(EngineError {}));
            }
        };

        let hash = format!("{:x}", md5::compute(&migration));
        let file_name = format!("{}", &file.display());

        // Store in migration table and commit
        match trx.exec_drop(&insert as &str, (&version, &hash, &file_name,)) {
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
        let mut del = String::from("DELETE FROM `");
        del.push_str(&self.migration_table_name);
        del.push_str("` WHERE `migration` = ?;");

        // Do the transaction
        let trx = self.client.start_transaction(TxOpts::default());
        if trx.is_err() {
            let err = trx.err().unwrap();
            crit!("Could not create a transaction: {}", err.to_string());
            return Err(Box::new(err));
        }

        // Executing migration
        let mut trx = trx.unwrap();
        match trx.query_drop(migration) {
            Ok(_) => {},
            Err(e) => {
                crit!("{}", e);
                //print_error_postgres(migration, e);
                return Err(Box::new(EngineError {}));
            }
        };

        // Store in migration table and commit
        match trx.exec_drop(&del as &str, (&version,)) {
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