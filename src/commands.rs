pub mod interactive;
pub mod down;
pub mod up;
pub mod create;
pub mod status;

use crate::{Configuration, EngineName};
use crate::filesystem::File;

/// Debug configuration & files.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
/// * `command` - The text for the files listing.
/// * `nothing` - If there is nothing to do, this will be printed instead of command.
/// * `files` - The files.
pub fn debug_configuration(configuration: &Configuration, command: &str, nothing: &str, files: &Vec<File>) {
    match configuration.engine {
        EngineName::POSTGRESQL => debug!("Engine: PostgreSQL"),
        EngineName::MYSQL => debug!("Engine: MySQL"),
        EngineName::SQLITE => debug!("Engine: SQLite"),
    };
    if configuration.url.len() > 0 {
        debug!("url: {}", &configuration.url);
    } else {
        debug!("host: {}", &configuration.host);
        debug!("port: {}", &configuration.port);
        debug!("database: {}", &configuration.database);
        debug!("username: {}", &configuration.username);
        debug!("password: {}", &configuration.password);
    }
    debug!("table: {}", &configuration.table);
    debug!("continue on error: {}", &configuration.continue_on_error);

    if files.len() == 0 {
        if nothing.len() > 0 {
            debug!("{}", nothing);
        }
    } else {
        debug!("{}", command);
        for file in files {
            debug!("{}", file.origin.display());
        }
    }
}