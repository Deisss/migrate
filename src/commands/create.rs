use crate::Configuration;
use crate::EngineName;
use crate::CreateType;
use std::fs::create_dir_all;
use std::path::{PathBuf, Path};
use std::io::{stdin, stdout, Write};
use chrono::prelude::*;
use std::fs::File;
use regex::RegexBuilder;
use std::error::Error;

// The current time
struct CurrentTime {
    year: String,
    month: String,
    day: String,
    hour: String,
    minute: String,
    second: String
}

/// Check if folder exists or not, if not, ask user.
///
/// # Arguments
///
/// * `path` - The folder to check
fn ask_for_new_folder(configuration: &Configuration, path: &str) -> bool {
    if configuration.debug == true {
        return true;
    }
    println!("The folder {} doesn't exists", path);
    print!("Should it be created? [Y/n]:");
    let _flush = stdout().flush();
    let mut s = String::new();
    let res = stdin().read_line(&mut s);
    s = s.trim().to_string();

    // Extracting migration
    if !res.is_err() && (s == "Y" || s == "y" || s == "") {
        return true;
    }

    false
}

/// Get the current time.
///
/// # Arguments
///
/// * `path` - The folder to check
fn get_current_time() -> CurrentTime {
    let local: DateTime<Local> = Local::now();
    CurrentTime {
        year:  format!("{:04}", local.year()),
        month: format!("{:02}", local.month()),
        day: format!("{:02}", local.day()),
        hour: format!("{:02}", local.hour()),
        minute: format!("{:02}", local.minute()),
        second: format!("{:02}", local.second())
    }
}

/// Create migration folder if not existing.
///
/// # Arguments
///
/// * `path` - The folder to create.
fn create_folder(configuration: &Configuration, path: &str) -> bool {
    if configuration.debug == true {
        return true;
    }
    let result = create_dir_all(path);
    if result.is_err() {
        crit!("Could not create migration folder: {}", result.err().unwrap());
        return false;
    }
    return true;
}

/// Write the migration file.
///
/// # Arguments
///
/// * `filename` - The filename to write into.
/// * `content` - The content to set.
fn create_file(filename: &PathBuf, content: &str) {
    let file = File::create(filename);
    if file.is_err() {
        crit!("Could not create file: {}", file.err().unwrap());
    } else {
        let mut file = file.ok().unwrap();
        let res = write!(file, "{}", content);
        if res.is_err() {
            crit!("Could not write to file: {}", res.err().unwrap());
        }
    }
}

/// Try to extract some information out of given regex.
///
/// # Arguments
///
/// * `regex` - The regex to use.
/// * `content` - The content to extract from.
fn try_to_extract(regex: &str, content: &str) -> Result<(String, String), Box<dyn Error>> {
    let re = RegexBuilder::new(regex).case_insensitive(true).build()?;
    let data = re.captures(content);
    if data.is_none() {
        return Ok((String::new(), String::new()));
    }
    let data = data.unwrap();
    if let Some(table_name) = data.name("name") {
        if let Some(column_name) = data.name("column") {
            return Ok((String::from(table_name.as_str()), String::from(column_name.as_str())));
        }
        return Ok((String::from(table_name.as_str()), String::new()));
    }
    Ok((String::new(), String::new()))
}

/// Get sample code for table creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The table name.
fn get_sample_create_table(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => return format!("CREATE TABLE `{}` (\n\t`id` INT NOT NULL AUTO_INCREMENT PRIMARY KEY\n);", &name),
        EngineName::SQLITE => return format!("CREATE TABLE \"{}\" (\n\t\"id\" INTEGER PRIMARY KEY AUTOINCREMENT\n);", &name),
        EngineName::POSTGRESQL => return format!("CREATE TABLE \"{}\" (\n\t\"id\" SERIAL PRIMARY KEY\n);", &name),
    };
}

/// Get sample code for table deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The table name.
fn get_sample_drop_table(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => return format!("DROP TABLE IF EXISTS `{}`;", &name),
        EngineName::SQLITE | EngineName::POSTGRESQL => return format!("DROP TABLE IF EXISTS \"{}\";", &name),
    };
}

/// Get sample code for column creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `column_name` - The column name.
fn get_sample_create_column(engine: &EngineName, table_name: &str, column_name: &str) -> String {
    let mut column_name = String::from(column_name);
    while column_name.ends_with("_") {
        column_name.truncate(column_name.len() - 1);
    }
    match engine {
        EngineName::MYSQL => return format!("ALTER TABLE `{}` ADD COLUMN `{}` VARCHAR(255);", table_name, &column_name),
        EngineName::SQLITE | EngineName::POSTGRESQL => return format!("ALTER TABLE \"{}\" ADD COLUMN \"{}\" TEXT;", table_name, &column_name),
    };
}

/// Get sample code for column deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `column_name` - The column name.
fn get_sample_drop_column(engine: &EngineName, table_name: &str, column_name: &str) -> String {
    let mut column_name = String::from(column_name);
    while column_name.ends_with("_") {
        column_name.truncate(column_name.len() - 1);
    }
    match engine {
        EngineName::MYSQL => return format!("ALTER TABLE `{}` DROP `{}`;", table_name, &column_name),
        EngineName::POSTGRESQL => return format!("ALTER TABLE \"{}\" DROP COLUMN \"{}\";", table_name, &column_name),
        // SQLite we, on purpose, do nothing
        EngineName::SQLITE => return String::from("")
    };
}

/// Get sample code for index creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `index_name` - The index name.
fn get_sample_create_index(engine: &EngineName, table_name: &str, index_name: &str) -> String {
    let mut index_name = String::from(index_name);
    while index_name.ends_with("_") {
        index_name.truncate(index_name.len() - 1);
    }
    match engine {
        EngineName::MYSQL | EngineName::SQLITE | EngineName::POSTGRESQL => return format!("CREATE INDEX \"idx_{}_{}\" ON \"{}\"(\"{}\");", table_name, &index_name, table_name, &index_name),
    };
}

/// Get sample code for index deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `index_name` - The index name.
fn get_sample_drop_index(engine: &EngineName, table_name: &str, index_name: &str) -> String {
    let mut index_name = String::from(index_name);
    while index_name.ends_with("_") {
        index_name.truncate(index_name.len() - 1);
    }
    match engine {
        EngineName::MYSQL | EngineName::SQLITE | EngineName::POSTGRESQL => return format!("DROP INDEX IF EXISTS \"idx_{}_{}\";", table_name, &index_name),
    };
}

/// Try to generate a sample of the asked up command.
///
/// # Arguments
///
/// * `configuration` - The configuration.
fn get_sample(mode: usize, configuration: &Configuration) -> String {
    let s = configuration.create_name.clone();

    // Create table command
    match try_to_extract(r"^(create|add)_?table_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_table(&configuration.engine, &name);
                } else {
                    return get_sample_drop_table(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove table
    match try_to_extract(r"^(remove|drop)_?table_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_table(&configuration.engine, &name);
                } else {
                    return get_sample_create_table(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Add column command
    match try_to_extract(r"^(create|add)_?column_?(?P<column>[a-zA-Z0-9\-_]+)_?to_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((table_name, column_name)) => {
            if table_name.len() > 0 && column_name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_column(&configuration.engine, &table_name, &column_name);
                } else {
                    let res = get_sample_drop_column(&configuration.engine, &table_name, &column_name);
                    if res.len() > 0 {
                        return res;
                    }
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove/drop column
    match try_to_extract(r"^(remove|drop)_?column_?(?P<column>[a-zA-Z0-9\-_]+)_?from_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((table_name, column_name)) => {
            if table_name.len() > 0 && column_name.len() > 0 {
                if mode == 0 {
                    let res = get_sample_drop_column(&configuration.engine, &table_name, &column_name);
                    if res.len() > 0 {
                        return res;
                    }
                } else {
                    return get_sample_create_column(&configuration.engine, &table_name, &column_name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Create index
    match try_to_extract(r"^(create|add)_?index_?for_?(?P<column>[a-zA-Z0-9\-_]+)_?on_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((table_name, column_name)) => {
            if table_name.len() > 0 && column_name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_index(&configuration.engine, &table_name, &column_name);
                } else {
                    return get_sample_drop_index(&configuration.engine, &table_name, &column_name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // remove/drop index
    match try_to_extract(r"^(remove|drop)_?index_?for_?(?P<column>[a-zA-Z0-9\-_]+)_?on_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((table_name, column_name)) => {
            if table_name.len() > 0 && column_name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_index(&configuration.engine, &table_name, &column_name);
                } else {
                    return get_sample_create_index(&configuration.engine, &table_name, &column_name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    if mode == 0 {
        return String::from("-- Your migration goes here");
    } else {
        return String::from("-- Your revert goes here");
    }
}

/// Generate the sample content within the file.
///
/// # Arguments
///
/// * `folder` - The folder to put migration into.
/// * `configuration` - The migration configuration.
fn get_file_content(t: usize, configuration: &Configuration, time: &CurrentTime) -> String {
    let mut s: String = String::new();
    let mut up_command = String::new();
    let mut down_command = String::new();
    let up_sample = get_sample(0, &configuration);
    let down_sample = get_sample(1, &configuration);

    if configuration.create_type == CreateType::FILE {
        up_command.push_str("-- ====  UP  ====\n");
        down_command.push_str("-- ==== DOWN ====\n");
    }

    // Up command (or single file)
    if configuration.create_type == CreateType::FILE || t == 1 {
        s.push_str(&format!("-- Migration: {}\n-- Created at: {}-{}-{} {}:{}:{}\n{}\n{}\n",
            &configuration.create_name, &time.year, &time.month, &time.day, &time.hour,
            &time.minute, &time.second, &up_command, &up_sample));
    } else if t == 2 {
        s.push_str(&format!("-- Migration: {}\n-- Created at: {}-{}-{} {}:{}:{}\n{}\n{}\n",
            &configuration.create_name, &time.year, &time.month, &time.day, &time.hour,
            &time.minute, &time.second, &down_command, &down_sample));
    }

    // Down command (or single file)
    if configuration.create_type == CreateType::FILE {
        s.push('\n');
        s.push_str(&format!("{}\n{}\n",&down_command, &down_sample));
    }

    s
}

/// Debug the configuration content.
///
/// # Arguments
///
/// * `configuration` - The migration configuration.
fn debug_configuration(configuration: &Configuration) {
    match configuration.engine {
        EngineName::POSTGRESQL => debug!("Engine: PostgreSQL"),
        EngineName::MYSQL => debug!("Engine: MySQL"),
        EngineName::SQLITE => debug!("Engine: SQLite"),
    };
}

/// Create the migration file.
///
/// # Arguments
///
/// * `folder` - The folder to put migration into.
/// * `configuration` - The migration configuration.
fn process_create(folder: &str, configuration: &Configuration) {
    let t = get_current_time();

    // Now is YYYYMMDDhhmmss
    let now = &[&t.year as &str, &t.month as &str, &t.day as &str, &t.hour as &str, &t.minute as &str, &t.second as &str].join("");

    if configuration.create_type == CreateType::FILE {
        let filename = &[now, "_", &configuration.create_name, ".sql"].join("");
        let full_filename = Path::new(folder).join(filename);
        if configuration.debug == true {
            debug_configuration(configuration);
            debug!("File to be created:");
            debug!("{}", full_filename.display());
        } else {
            create_file(&full_filename, &get_file_content(0, &configuration, &t));
        }

    } else if configuration.create_type == CreateType::FOLDER {
        let full_folder = Path::new(folder).join(&[now, "_", &configuration.create_name].join(""));
        let full_folder_str = full_folder.clone().into_os_string().into_string();
        if full_folder_str.is_err() {
            crit!("Could not create migration folder: {}", full_folder_str.clone().err().unwrap().into_string().unwrap());
        }
        let full_folder_str = full_folder_str.ok().unwrap();
        if create_folder(&configuration, &full_folder_str) == true {
            let full_filename_up = full_folder.join("up.sql");
            let full_filename_down = full_folder.join("down.sql");
            if configuration.debug == true {
                debug_configuration(configuration);
                debug!("Files to be created:");
                debug!("{}", full_filename_up.display());
                debug!("{}", full_filename_down.display());
            } else {
                create_file(&full_filename_up, &get_file_content(1, &configuration, &t));
                create_file(&full_filename_down, &get_file_content(2, &configuration, &t));
            }
        }

    } else if configuration.create_type == CreateType::SPLITFILES {
        let full_filename_up = Path::new(folder).join(&[now, "_", &configuration.create_name, ".up.sql"].join(""));
        let full_filename_down = Path::new(folder).join(&[now, "_", &configuration.create_name, ".down.sql"].join(""));
        if configuration.debug == true {
            debug_configuration(configuration);
            debug!("Files to be created:");
            debug!("{}", full_filename_up.display());
            debug!("{}", full_filename_down.display());
        } else {
            create_file(&full_filename_up, &get_file_content(1, &configuration, &t));
            create_file(&full_filename_down, &get_file_content(2, &configuration, &t));
        }
    }
}

/// Create new migration file.
///
/// # Arguments
///
/// * `configuration` - The configuration to use.
pub fn process(configuration: &Configuration) -> bool {
    let migration_folder = &configuration.path;

    if Path::new(&migration_folder).exists() == true {
        process_create(&migration_folder, &configuration);
    } else if ask_for_new_folder(&configuration, &migration_folder) == true {
        if create_folder(&configuration, &migration_folder) == true {
            process_create(&migration_folder, &configuration);
        }
    }

    true
}