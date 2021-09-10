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

macro_rules! trim_underscore {
    ($input:ident) => {
        {
            let mut s = String::from($input);
            while s.ends_with("_") {
                s.truncate(s.len() - 1);
            }
            s
        }
    }
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

    // If there is no error and it's a "yes" we send back true, otherwise false...
    !res.is_err() && (s == "Y" || s == "y" || s == "")
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
    match configuration.debug {
        true => true,
        false => match create_dir_all(path) {
            Ok(_) => true,
            Err(e) => {
                crit!("Could not create migration folder: {}", e);
                false
            }
        }
    }
}

/// Write the migration file.
///
/// # Arguments
///
/// * `filename` - The filename to write into.
/// * `content` - The content to set.
fn create_file(filename: &PathBuf, content: &str) {
    match File::create(filename) {
        Ok(mut file) => {
            match write!(file, "{}", content) {
                Err(e) => crit!("Could not write to file: {}", e),
                _ => {}
            }
        },
        Err(e) => crit!("Could not create file: {}", e)
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

    match data {
        Some(data) => {
            if let Some(table_name) = data.name("name") {
                if let Some(column_name) = data.name("column") {
                    return Ok((String::from(table_name.as_str()), String::from(column_name.as_str())));
                }
                return Ok((String::from(table_name.as_str()), String::new()));
            }
            Ok((String::new(), String::new()))
        },
        None => Ok((String::new(), String::new()))
    }
}

/// Get sample code for table creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The table name.
fn get_sample_create_table(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => format!("CREATE TABLE `{}` (\n\t`id` INT NOT NULL AUTO_INCREMENT PRIMARY KEY\n);", &name),
        EngineName::SQLITE => format!("CREATE TABLE \"{}\" (\n\t\"id\" INTEGER PRIMARY KEY AUTOINCREMENT\n);", &name),
        EngineName::POSTGRESQL => format!("CREATE TABLE \"{}\" (\n\t\"id\" SERIAL PRIMARY KEY\n);", &name),
    }
}

/// Get sample code for table deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The table name.
fn get_sample_drop_table(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => format!("DROP TABLE IF EXISTS `{}`;", &name),
        EngineName::SQLITE | EngineName::POSTGRESQL => format!("DROP TABLE IF EXISTS \"{}\";", &name),
    }
}

/// Get sample code for column creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `column_name` - The column name.
fn get_sample_create_column(engine: &EngineName, table_name: &str, column_name: &str) -> String {
    let column_name = trim_underscore!(column_name);
    match engine {
        EngineName::MYSQL => format!("ALTER TABLE `{}` ADD COLUMN `{}` VARCHAR(255);", table_name, &column_name),
        EngineName::SQLITE | EngineName::POSTGRESQL => format!("ALTER TABLE \"{}\" ADD COLUMN \"{}\" TEXT;", table_name, &column_name),
    }
}

/// Get sample code for column deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `column_name` - The column name.
fn get_sample_drop_column(engine: &EngineName, table_name: &str, column_name: &str) -> String {
    let column_name = trim_underscore!(column_name);
    match engine {
        EngineName::MYSQL => format!("ALTER TABLE `{}` DROP `{}`;", table_name, &column_name),
        EngineName::POSTGRESQL => format!("ALTER TABLE \"{}\" DROP COLUMN \"{}\";", table_name, &column_name),
        // SQLite we, on purpose, do nothing
        EngineName::SQLITE => String::from("")
    }
}

/// Get sample code for index creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `index_name` - The index name.
fn get_sample_create_index(engine: &EngineName, table_name: &str, index_name: &str) -> String {
    let index_name = trim_underscore!(index_name);
    match engine {
        EngineName::MYSQL | EngineName::SQLITE | EngineName::POSTGRESQL => format!("CREATE INDEX \"idx_{}_{}\" ON \"{}\"(\"{}\");", table_name, &index_name, table_name, &index_name),
    }
}

/// Get sample code for index deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `table_name` - The table name.
/// * `index_name` - The index name.
fn get_sample_drop_index(engine: &EngineName, table_name: &str, index_name: &str) -> String {
    let index_name = trim_underscore!(index_name);
    match engine {
        EngineName::MYSQL | EngineName::SQLITE | EngineName::POSTGRESQL => format!("DROP INDEX IF EXISTS \"idx_{}_{}\";", table_name, &index_name),
    }
}

/// Get sample code for function creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The function name.
fn get_sample_create_function(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => format!("DELIMITER $$\nCREATE FUNCTION `{}`()\nRETURNS decimal\nDETERMINISTIC\nBEGIN\nRETURN 10;\nEND$$\nDELIMITER;", &name),
        EngineName::SQLITE => String::from("-- SQLite doesn't support SQL functions"),
        EngineName::POSTGRESQL => format!("CREATE OR REPLACE FUNCTION \"{}\"() RETURNS void AS $func$\nDECLARE\nBEGIN\nEND\n$func$ LANGUAGE plpgsql;", &name),
    }
}

/// Get sample code for function deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The function name.
fn get_sample_drop_function(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => format!("DROP FUNCTION IF EXISTS `{}`;", &name),
        EngineName::SQLITE => String::from("-- SQLite doesn't support SQL functions"),
        EngineName::POSTGRESQL => format!("DROP FUNCTION IF EXISTS \"{}\"();", &name),
    }
}

/// Get sample code for enum creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The enum name.
fn get_sample_create_enum(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support user defined types"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support user defined types"),
        EngineName::POSTGRESQL => format!("CREATE TYPE \"{}\" AS ENUM (\n    'first',\n    'second'\n);", &name),
    }
}

/// Get sample code for enum deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The enum name.
fn get_sample_drop_enum(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support user defined types"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support user defined types"),
        EngineName::POSTGRESQL => format!("DROP TYPE IF EXISTS \"{}\";", &name),
    }
}

/// Get sample code for type creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The type name.
fn get_sample_create_type(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support user defined types"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support user defined types"),
        EngineName::POSTGRESQL => format!("CREATE TYPE \"{}\" AS (\n    \"property1\" INT,\n    \"property2\" TEXT\n);", &name),
    }
}

/// Get sample code for type deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The type name.
fn get_sample_drop_type(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support user defined types"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support user defined types"),
        EngineName::POSTGRESQL => format!("DROP TYPE IF EXISTS \"{}\";", &name),
    }
}

/// Get sample code for domain creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The domain name.
fn get_sample_create_domain(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support domain"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support domain"),
        EngineName::POSTGRESQL => format!("CREATE DOMAIN \"{}\" INT CHECK (VALUE > 0 AND VALUE < 999);", &name),
    }
}

/// Get sample code for domain deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The domain name.
fn get_sample_drop_domain(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support domain"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support domain"),
        EngineName::POSTGRESQL => format!("DROP DOMAIN IF EXISTS \"{}\";", &name),
    }
}

/// Get sample code for view creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The view name.
fn get_sample_create_view(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => format!("CREATE OR REPLACE VIEW `{}` AS SELECT 'Hello World' AS `hello`", &name),
        EngineName::SQLITE => format!("CREATE VIEW \"{}\" AS SELECT 'Hello World' AS \"hello\"", &name),
        EngineName::POSTGRESQL => format!("CREATE OR REPLACE VIEW \"{}\" AS SELECT text 'Hello World' AS \"hello\";", &name),
    }
}

/// Get sample code for view deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The view name.
fn get_sample_drop_view(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => format!("DROP VIEW IF EXISTS `{}`", &name),
        EngineName::SQLITE => format!("DROP VIEW IF EXISTS \"{}\"", &name),
        EngineName::POSTGRESQL => format!("DROP VIEW IF EXISTS \"{}\";", &name),
    }
}

/// Get sample code for materialized view creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The materialized view name.
fn get_sample_create_materialized_view(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support materialized view"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support materialized view"),
        EngineName::POSTGRESQL => format!("CREATE MATERIALIZED VIEW \"{}\" AS SELECT text 'Hello World' AS \"hello\";", &name),
    }
}

/// Get sample code for materialized view deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `name` - The materialized view name.
fn get_sample_drop_materialized_view(engine: &EngineName, name: &str) -> String {
    match engine {
        EngineName::MYSQL => String::from("-- MySQL doesn't support materialized view"),
        EngineName::SQLITE => String::from("-- SQLite doesn't support materialized view"),
        EngineName::POSTGRESQL => format!("DROP MATERIALIZED VIEW IF EXISTS \"{}\";", &name),
    }
}

/// Get sample code for trigger creation.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `trigger_name` - The trigger name.
/// * `table_name` - The table name.
fn get_sample_create_trigger(engine: &EngineName, trigger_name: &str, table_name: &str) -> String {
    let trigger_name = trim_underscore!(trigger_name);
    match engine {
        EngineName::MYSQL => format!("DELIMITER $$\n\nCREATE TRIGGER `{}`\n    AFTER INSERT\n    ON `{}` FOR EACH ROW\nBEGIN\n    -- statements\nEND$$\n\nDELIMITER ;", trigger_name, &table_name),
        EngineName::SQLITE => format!("CREATE TRIGGER IF NOT EXISTS \"{}\"\n    AFTER INSERT\n   ON \"{}\"\nBEGIN\n    -- statements\nEND;", trigger_name, &table_name),
        EngineName::POSTGRESQL => format!("CREATE TRIGGER \"{}\"\n    AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE\n    ON \"{}\"\n    FOR EACH STATEMENT\nEXECUTE PROCEDURE my_function();", trigger_name, &table_name),
    }
}

/// Get sample code for trigger deletion.
///
/// # Arguments
///
/// * `engine` - The engine type.
/// * `trigger_name` - The trigger name.
/// * `table_name` - The table name.
fn get_sample_drop_trigger(engine: &EngineName, trigger_name: &str, table_name: &str) -> String {
    let trigger_name = trim_underscore!(trigger_name);
    match engine {
        EngineName::MYSQL => format!("DROP TRIGGER IF EXISTS `{}`;", trigger_name),
        EngineName::POSTGRESQL => format!("DROP TRIGGER IF EXISTS \"{}\" ON \"{}\";", trigger_name, &table_name),
        EngineName::SQLITE => format!("DROP TRIGGER IF EXISTS \"{}\";", trigger_name),
    }
}

/// Try to generate a sample of the asked up command.
///
/// # Arguments
///
/// * `configuration` - The configuration.
fn get_sample(mode: usize, configuration: &Configuration) -> String {
    let s = configuration.create_name.clone();

    // Create table
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

    // Add column
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

    // Remove column
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

    // Remove index
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

    // Create function
    match try_to_extract(r"^(create|add)_?(function|plsql|psql)_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_function(&configuration.engine, &name);
                } else {
                    return get_sample_drop_function(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove function
    match try_to_extract(r"^(remove|drop)_?(function|plsql|psql)_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_function(&configuration.engine, &name);
                } else {
                    return get_sample_create_function(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Create enum
    match try_to_extract(r"^(create|add)_?enum_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_enum(&configuration.engine, &name);
                } else {
                    return get_sample_drop_enum(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove enum
    match try_to_extract(r"^(remove|drop)_?enum_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_enum(&configuration.engine, &name);
                } else {
                    return get_sample_create_enum(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Create type
    match try_to_extract(r"^(create|add)_?type_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_type(&configuration.engine, &name);
                } else {
                    return get_sample_drop_type(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove type
    match try_to_extract(r"^(remove|drop)_?type_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_type(&configuration.engine, &name);
                } else {
                    return get_sample_create_type(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Create domain
    match try_to_extract(r"^(create|add)_?domain_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_domain(&configuration.engine, &name);
                } else {
                    return get_sample_drop_domain(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove domain
    match try_to_extract(r"^(remove|drop)_?domain_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_domain(&configuration.engine, &name);
                } else {
                    return get_sample_create_domain(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Create view
    match try_to_extract(r"^(create|add)_?view_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_view(&configuration.engine, &name);
                } else {
                    return get_sample_drop_view(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove view
    match try_to_extract(r"^(remove|drop)_?view_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_view(&configuration.engine, &name);
                } else {
                    return get_sample_create_view(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Create materialized view
    match try_to_extract(r"^(create|add)_?materialized_view_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_materialized_view(&configuration.engine, &name);
                } else {
                    return get_sample_drop_materialized_view(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove materialized view
    match try_to_extract(r"^(remove|drop)_?materialized_view_?(?P<name>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((name, _)) => {
            if name.len() > 0 {
                if mode == 0 {
                    return get_sample_drop_materialized_view(&configuration.engine, &name);
                } else {
                    return get_sample_create_materialized_view(&configuration.engine, &name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Create trigger
    match try_to_extract(r"^(create|add)_?trigger_?(?P<name>[a-zA-Z0-9\-_]+)_?on_?(?P<table>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((trigger_name, table_name)) => {
            if trigger_name.len() > 0 && table_name.len() > 0 {
                if mode == 0 {
                    return get_sample_create_trigger(&configuration.engine, &trigger_name, &table_name);
                } else {
                    let res = get_sample_drop_trigger(&configuration.engine, &trigger_name, &table_name);
                    if res.len() > 0 {
                        return res;
                    }
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    // Remove trigger
    match try_to_extract(r"^(remove|drop)_?trigger_?(?P<name>[a-zA-Z0-9\-_]+)_?on_?(?P<table>[a-zA-Z0-9\-_]+)$", &s) {
        Ok((trigger_name, table_name)) => {
            if trigger_name.len() > 0 && table_name.len() > 0 {
                if mode == 0 {
                    let res = get_sample_drop_trigger(&configuration.engine, &trigger_name, &table_name);
                    if res.len() > 0 {
                        return res;
                    }
                } else {
                    return get_sample_create_trigger(&configuration.engine, &trigger_name, &table_name);
                }
            }
        },
        Err(e) => crit!("{}", e),
    };

    match mode {
        0 => String::from("-- Your migration goes here"),
        _ => String::from("-- Your revert goes here")
    }
}

/// Generate the sample content within the file.
///
/// # Arguments
///
/// * `folder` - The folder to put migration into.
/// * `configuration` - The migration configuration.
fn get_file_content(t: usize, configuration: &Configuration) -> String {
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
        s.push_str(&format!("{}\n{}\n", &up_command, &up_sample));
    } else if t == 2 {
        s.push_str(&format!("{}\n{}\n", &down_command, &down_sample));
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
    let now = format!("{}{}{}{}{}{}", &t.year, &t.month, &t.day, &t.hour, &t.minute, &t.second);

    match configuration.create_type {
        CreateType::FILE => {
            let filename = &[&now, "_", &configuration.create_name, ".sql"].join("");
            let full_filename = Path::new(folder).join(filename);
            if configuration.debug == true {
                debug_configuration(configuration);
                debug!("File to be created:");
                debug!("{}", full_filename.display());
            } else {
                create_file(&full_filename, &get_file_content(0, &configuration));
            }
        },
        CreateType::FOLDER => {
            let full_folder = Path::new(folder).join(&[&now, "_", &configuration.create_name].join(""));
            let full_folder_str = match full_folder.clone().into_os_string().into_string() {
                Ok(s) => s,
                Err(e) => {
                    crit!("Could not create migration folder: {}", e.into_string().unwrap());
                    return;
                }
            };

            if create_folder(&configuration, &full_folder_str) == true {
                let full_filename_up = full_folder.join("up.sql");
                let full_filename_down = full_folder.join("down.sql");

                match configuration.debug {
                    true => {
                        debug_configuration(configuration);
                        debug!("Files to be created:");
                        debug!("{}", full_filename_up.display());
                        debug!("{}", full_filename_down.display());
                    },
                    false => {
                        create_file(&full_filename_up, &get_file_content(1, &configuration));
                        create_file(&full_filename_down, &get_file_content(2, &configuration));
                    }
                };
            }
        },
        CreateType::SPLITFILES => {
            let full_filename_up = Path::new(folder).join(&[&now, "_", &configuration.create_name, ".up.sql"].join(""));
            let full_filename_down = Path::new(folder).join(&[&now, "_", &configuration.create_name, ".down.sql"].join(""));

            match configuration.debug {
                true => {
                    debug_configuration(configuration);
                    debug!("Files to be created:");
                    debug!("{}", full_filename_up.display());
                    debug!("{}", full_filename_down.display());
                },
                false => {
                    create_file(&full_filename_up, &get_file_content(1, &configuration));
                    create_file(&full_filename_down, &get_file_content(2, &configuration));
                }
            };
        }
    };
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