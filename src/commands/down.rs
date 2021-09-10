use crate::Configuration;
use crate::EngineName;

use crate::helpers::{readable_time, skip_transaction};
use crate::engines::{get_sql_engine, EngineError};
use crate::filesystem::{File, get_sql, migrations, get_file_path_without_migration_path};
use super::debug_configuration;
use std::error::Error;
use std::time::Instant;

/// Revert one or more migrations.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
/// * `files` - The files found.
pub fn process_down_sql(configuration: &Configuration, files: &mut Vec<File>) -> Result<(), Box<dyn Error>> {
    let mut db = match get_sql_engine(&configuration.engine, configuration) {
        Ok(db) => db,
        Err(e) => {
            crit!("Error getting engine: {:?}", e);
            return Err(Box::new(EngineError {}));
        }
    };

    match db.create_migration_table() {
        Err(e) => {
            crit!("Error creating migration table: {:?}", e);
            return Err(Box::new(EngineError {}));
        },
        _ => {}
    };

    let existing = match db.get_migrations() {
        Ok(mut e) => {
            if configuration.step > 0 {
                e.truncate(configuration.step as usize);
            }
            e
        },
        Err(e) => {
            crit!("Error getting migrations: {:?}", e);
            return Err(Box::new(EngineError {}));
        }
    };

    // We keep the ones that we can revert
    files.retain(|file| existing.contains(&file.number.to_string()));

    // We debug and exit
    if configuration.debug == true {
        debug_configuration(&configuration, "Files to be reverted:", "Nothing to revert", &files);
        return Ok(());
    }

    // We migrate
    for file in files {
        let now = Instant::now();
        let file_name = get_file_path_without_migration_path(&configuration.path, &file.origin.display().to_string());
        info!("{} -> reverting", &file_name);

        let error: bool = match get_sql(&file, 0) {
            Ok(sql) => {
                match db.rollback(&file.origin, &file.number.to_string(), &sql, skip_transaction(&configuration, &sql)) {
                    Err(_e) => true,
                    _ => false
                }
            },
            Err(e) => {
                warn!("{} failed to read: {}", &file_name, e);
                true
            }
        };

        let elapsed = now.elapsed().as_millis();
        if error {
            let debug = format!("{} -> error after {}", &file_name, &readable_time(elapsed));
            crit!("{}", debug);
        } else {
            let debug = format!("{} -> migrated in {}", &file_name, &readable_time(elapsed));
            info!("{}", debug);
        }

        debug!("");

        // If the continue on error is set to false, we have to exit there.
        if error && configuration.continue_on_error == false {
            return Err(Box::new(EngineError {}));
        }
    }

    Ok(())
}

/// Process a migration.
///
/// # Arguments
///
/// * `configuration` - The configuration to use
pub fn process(configuration: &Configuration) -> bool {
    let mut files = migrations(&configuration.path, None);

    if files.len() == 0 {
        info!("Nothing to revert");
        return true;
    }

    // Filtering for version control
    if configuration.version.len() > 0 {
        // Filtering only the right element
        files.retain(|file| file.number.to_string() == configuration.version);
    }


    // We don't want to keep "down" files & we sort
    files.retain(|file| file.is_down);
    files.sort_by(|f1, f2| f2.partial_cmp(f1).unwrap());

    match files.len() {
        0 => {
            info!("Nothing to revert");
            true
        },
        _ => match configuration.engine {
            EngineName::POSTGRESQL | EngineName::SQLITE | EngineName::MYSQL => {
                match process_down_sql(configuration, &mut files) {
                    Err(_e) => false,
                    _ => true
                }
            }
        }
    }
}