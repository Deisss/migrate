use crate::Configuration;
use crate::EngineName;
use crate::helpers::{readable_time, skip_transaction};
use crate::engines::{get_sql_engine, EngineError};
use crate::filesystem::{File, get_sql, migrations, get_file_path_without_migration_path};
use super::debug_configuration;
use std::error::Error;
use std::time::Instant;

/// Do the migration.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
/// * `files` - The files found.
pub fn process_up_sql(configuration: &Configuration, files: &mut Vec<File>) -> Result<(), Box<dyn Error>> {
    let db = get_sql_engine(&configuration.engine, configuration);
    if db.is_err() {
        crit!("Error getting engine: {:?}", db.as_ref().err());
    }
    let mut db = db.unwrap();

    match db.create_migration_table() {
        Err(e) => {
            crit!("Error creating migration table: {:?}", e);
        },
        _ => {}
    };

    let existing = db.get_migrations();
    if existing.is_err() {
        crit!("Error getting migrations: {:?}", existing.as_ref().err());
    }
    let existing = existing.unwrap();

    // We keep the ones that we can migrate
    files.retain(|file| !existing.contains(&file.number.to_string()));

    if configuration.step > 0 {
        files.truncate(configuration.step as usize);
    }

    // We debug and exit
    if configuration.debug == true {
        debug_configuration(&configuration, "Files to be migrated:", "Nothing to migrate", &files);
        return Ok(());
    }

    // We migrate
    for file in files {
        let now = Instant::now();
        let file_name = get_file_path_without_migration_path(&configuration.path, &file.origin.display().to_string());
        info!("{} -> migrating", &file_name);
        let error: bool = match get_sql(&file, 1) {
            Ok(sql) => {
                match db.migrate(&file.origin, &file.number.to_string(), &configuration.migration_type, &sql, skip_transaction(&configuration, &sql)) {
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
        info!("Nothing to migrate");
        return true;
    }

    // Filtering for version control
    if configuration.version.len() > 0 {
        // Filtering only the right element
        files.retain(|file| file.number.to_string() == configuration.version);
    }

    // We don't want to keep "up" files & we sort
    files.retain(|file| file.is_up);
    files.sort_by(|f1, f2| f1.partial_cmp(f2).unwrap());

    if files.len() == 0 {
        info!("Nothing to migrate");
        return true;
    }

    match configuration.engine {
        EngineName::POSTGRESQL | EngineName::SQLITE | EngineName::MYSQL => {
            match process_up_sql(configuration, &mut files) {
                Err(_e) => false,
                _ => true
            }
        }
    }
}