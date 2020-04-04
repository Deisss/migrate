use crate::filesystem::{self, File, get_file_path_without_migration_path};
use crate::Configuration;
use crate::EngineName;
use crate::engines::get_sql_engine;
use crate::commands::interactive::{merge_migrations_and_files, InteractiveMigration, InteractionType};
use crate::helpers::{limit_number, limit_per_date};
use console::Style;
use std::error::Error;

/// Show the status.
///
/// # Arguments
///
/// * `root` - The root folder where all migrations are.
/// * `migrations` - The files & migrations.
fn show_status(root: &str, migrations: &mut Vec<InteractiveMigration>) {
    let installed = Style::new().green();
    let notinstalled = Style::new().red();
    let installed_with_warning = Style::new().yellow();
    let inactive = Style::new().dim();
    let yellow = Style::new().yellow();

    println!("");
    println!("Installed | migration number | name");
    println!("----------+------------------+----------------------------");

    for index in 0..migrations.len() {
        if let Some(migration) = migrations.get(index) {
            let mut content = String::new();
    
            if migration.current_type == InteractionType::UP {
                let m_hash = migration.migration_hash.as_ref();
                let f_hash = migration.file_up_hash.as_ref();
                if m_hash.is_some() && f_hash.is_some() && Some(m_hash) == Some(f_hash) {
                    content.push_str(&format!("   {}    ", installed.apply_to("yes")));
                } else {
                    content.push_str(&format!(" {}  ", installed_with_warning.apply_to("changed")));
                }
                
            } else {
                content.push_str(&format!("   {}     ", notinstalled.apply_to("no")));
            }

            content.push_str("| ");
            content.push_str(&limit_number(&migration.number));
            content.push_str(" | ");
    
            if migration.file_up.is_some() {
                let f = migration.file_up.as_ref().unwrap();
                let file_name = get_file_path_without_migration_path(root, &f.origin.display().to_string());
                content.push_str(&format!("{} {}{}{}", f.name, inactive.apply_to("("), inactive.apply_to(file_name), inactive.apply_to(")")));
            } else if migration.migration_origin.is_some() {
                content.push_str(&format!("{} {}was: {}{}", yellow.apply_to("missing file"),
                    inactive.apply_to("("), inactive.apply_to(migration.migration_origin.as_ref().unwrap()),
                    inactive.apply_to(")")
                ));
            }
            println!("{}", &content.replace("\"", ""));
        }
    }

    println!("");
}

/// Do the status mode.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
/// * `files` - The files.
fn process_status_sql(configuration: &Configuration, files: &mut Vec<File>) -> Result<(), Box<dyn Error>> {
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

    let existing = db.get_migrations_with_hashes();
    if existing.is_err() {
        crit!("Error getting migrations: {:?}", existing.as_ref().err());
    }
    let mut existing = existing.unwrap();

    // Filtering files & existing if needed
    if configuration.interactive_days > 0 {
        existing.retain(|(migration, _, _)| limit_per_date(migration, configuration.interactive_days));
        files.retain(|file| limit_per_date(&file.number.to_string(), configuration.interactive_days));
    }

    let mut to_show = merge_migrations_and_files(&existing, files);
    show_status(&configuration.path, &mut to_show);

    Ok(())
}

/// Dump the status of the database.
///
/// # Arguments
///
/// * `configuration` - The configuration to use
pub fn process(configuration: &Configuration) -> bool {
    let mut files = filesystem::migrations(&configuration.path, None);
    files.sort_by(|f1, f2| f1.partial_cmp(f2).unwrap());

    match configuration.engine {
        EngineName::POSTGRESQL | EngineName::SQLITE | EngineName::MYSQL => {
            match process_status_sql(configuration, &mut files) {
                Err(_e) => false,
                _ => true
            }
        }
    }
}