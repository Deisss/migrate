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
/// * `logger` - Logger instance.
pub fn debug_configuration(configuration: &Configuration, command: &str, nothing: &str, files: &Vec<File>, logger: &slog::Logger) {
    match configuration.engine {
        EngineName::POSTGRESQL => slog::debug!(logger, "Engine: PostgreSQL"),
        EngineName::MYSQL => slog::debug!(logger, "Engine: MySQL"),
        EngineName::SQLITE => slog::debug!(logger, "Engine: SQLite"),
    };
    if configuration.url.len() > 0 {
        slog::debug!(logger, "url: {}", &configuration.url);
    } else {
        slog::debug!(logger, "host: {}", &configuration.host);
        slog::debug!(logger, "port: {}", &configuration.port);
        slog::debug!(logger, "database: {}", &configuration.database);
        slog::debug!(logger, "username: {}", &configuration.username);
        slog::debug!(logger, "password: {}", &configuration.password);
    }
    slog::debug!(logger, "table: {}", &configuration.table);
    slog::debug!(logger, "continue on error: {}", &configuration.continue_on_error);

    if files.len() == 0 {
        if nothing.len() > 0 {
            slog::debug!(logger, "{}", nothing);
        }
    } else {
        slog::debug!(logger, "{}", command);
        for file in files {
            slog::debug!(logger, "{}", file.origin.display());
        }
    }
}