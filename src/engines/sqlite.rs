use rusqlite::{Connection, NO_PARAMS};
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
        let connection = Connection::open(url);
        if connection.is_err() {
            let err = connection.err().unwrap();
            crit!("Could not open database for SqLite: {}", err.to_string());
            return Err(Box::new(err));
        }
        let connection = connection.unwrap();
        Ok(Box::new(Sqlite {
            client: connection,
            migration_table_name: migration_table_name.to_owned(),
        }))
    }
}

impl SqlEngine for Sqlite {
    fn create_migration_table(&mut self) -> Result<u64, Box<dyn Error>> {
        let mut create_table: String = String::from("CREATE TABLE IF NOT EXISTS \"");
        create_table.push_str(&self.migration_table_name);
        create_table.push_str("\" (\"migration\" TEXT PRIMARY KEY, \"hash\" TEXT, \"type\" TEXT, \"file_name\" TEXT, \"created_at\" TIMESTAMP)");
        match self.client.execute(&create_table as &str, NO_PARAMS) {
            Ok(_) => Ok(0),
            Err(e) => Err(Box::new(e))
        }
    }

    fn get_migrations(&mut self) -> Result<Vec<String>, Box<dyn Error>> {
        let mut results: Vec<String> = Vec::new();
        let mut get_migration = String::from("SELECT \"migration\" FROM \"");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("\" ORDER BY \"migration\" desc");
        let mut stmt = self.client.prepare(&get_migration as &str)?;
        stmt.query_map(NO_PARAMS, |row| {
            let tmp = row.get(0);
            if tmp.is_ok() {
                results.push(tmp.unwrap());
            }
            Ok(())
        })?;
        Ok(results)
    }

    fn get_migrations_with_hashes(&mut self, migration_type: &str) -> Result<Vec<(String, String, String)>, Box<dyn Error>> {
        let mut results: Vec<(String, String, String)> = Vec::new();
        let mut get_migration = String::from("SELECT \"migration\", \"hash\", \"file_name\" FROM \"");
        get_migration.push_str(&self.migration_table_name);
        get_migration.push_str("\" WHERE \"type\" = $1 ORDER BY \"migration\" desc");
        let mut stmt = self.client.prepare(&get_migration as &str)?;
        stmt.query_map(&[&migration_type], |row| {
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

    fn migrate(&mut self, file: &PathBuf, version: &str, migration_type: &str, migration: &str) -> Result<(), Box<dyn Error>> {
        // Insert statement
        let mut insert = String::from("INSERT INTO \"");
        insert.push_str(&self.migration_table_name);
        insert.push_str("\" (\"migration\", \"hash\", \"type\", \"file_name\", \"created_at\") VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP);");

        // Do the transaction
        let trx = self.client.transaction();
        if trx.is_err() {
            let err = trx.err().unwrap();
            crit!("Could not create a transaction: {}", err.to_string());
            return Err(Box::new(err));
        }

        let trx = trx.unwrap();
        match trx.execute(migration, NO_PARAMS) {
            Ok(_) => {},
            Err(e) => {
                println!("{:?}", e);
                return Err(Box::new(EngineError {}));
            }
        };

        let hash = format!("{:x}", md5::compute(&migration));
        let file_name = format!("{}", &file.display());

        // Store in migration table and commit
        match trx.execute(&insert as &str, &[&version, &hash[..], &migration_type, &file_name]) {
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

        let trx = trx.unwrap();
        match trx.execute(migration, NO_PARAMS) {
            Ok(_) => {},
            Err(e) => {
                println!("{:?}", e);
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
