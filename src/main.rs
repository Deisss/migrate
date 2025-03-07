mod filesystem;
mod commands;
mod engines;
mod helpers;

use commands::{interactive, up, down, create, status};
use std::default::Default;
use clap::{Arg, Command, ArgMatches};
use config::{Config, File};
use std::time::Instant;
use std::io::Write;
use console::Term;

#[macro_use]
extern crate slog;
#[macro_use]
extern crate slog_scope;
extern crate slog_async;
extern crate slog_term;
use slog::Drain;

/// Custom timestamp logger.
///
/// Arguments
///
/// * `io` - The writer.
pub fn timestamp_utc(io: &mut dyn Write) -> std::io::Result<()> {
    write!(io, "{}", chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ"))
}

#[derive(Debug, PartialEq)]
pub enum CommandName {
    UP,
    DOWN,
    INTERACTIVE,
    CREATE,
    STATUS,
}

impl Default for CommandName {
    fn default() -> Self { CommandName::UP }
}

#[derive(Debug, PartialEq)]
pub enum EngineName {
    POSTGRESQL,
    MYSQL,
    SQLITE,
}

impl Default for EngineName {
    fn default() -> Self { EngineName::POSTGRESQL }
}

#[derive(Debug, PartialEq)]
pub enum CreateType {
    FOLDER,
    FILE,
    SPLITFILES,
}

impl Default for CreateType {
    fn default() -> Self { CreateType::FOLDER }
}

#[derive(Debug, Default)]
pub struct Configuration {
    // Up, down & interactive
    command: CommandName,
    url: String,
    engine: EngineName,
    host: String,
    port: u32,
    database: String,
    username: String,
    password: String,
    table: String,
    path: String,
    interactive: bool,
    continue_on_error: bool,
    migration_type: String,
    version: String,
    step: u32,
    debug: bool,
    skip_transactions: bool,

    // Specific to interactive
    interactive_days: u32,

    // Specific to create
    create_name: String,
    create_type: CreateType,
}

/// Extract application parameters submitted by user (from configuration file only).
///
/// # Arguments
///
/// * `args` - Program args.
fn read_config_file(args: &ArgMatches) -> Configuration {
    // Get configuration file name
    let default_config_file = String::from("migration");
    let filename = args.get_one::<String>("config").unwrap_or(&default_config_file);

    // Loading file...
    let settings = Config::builder()
        .add_source(File::with_name(filename))
        .build().unwrap();

    let mut configuration: Configuration = Default::default();

    // Common configuration
    configuration.engine = match settings.get_string("engine") {
        Ok(s) => match &s[..] {
            "mysql" => EngineName::MYSQL,
            "sqlite" => EngineName::SQLITE,
            "postgres" | "postgresql" => EngineName::POSTGRESQL,
            // TODO: better error here...
            _ => EngineName::POSTGRESQL
        },
        _ => EngineName::POSTGRESQL
    };

    configuration.host = settings.get_string("host").unwrap_or(String::from("127.0.0.1"));
    configuration.table = settings.get_string("migration_table").unwrap_or(String::from("_schema_migration"));

    if configuration.engine == EngineName::POSTGRESQL {
        configuration.port = settings.get::<u32>("port").unwrap_or(6379);
        configuration.database = settings.get_string("database").unwrap_or(String::from("postgres"));
        configuration.username = settings.get_string("username").unwrap_or(String::from("postgres"));
        configuration.password = settings.get_string("password").unwrap_or(String::new());
    } else {
        configuration.port = settings.get::<u32>("port").unwrap_or(3306);
        configuration.database = settings.get_string("database").unwrap_or(String::from("mysql"));
        configuration.username = settings.get_string("username").unwrap_or(String::from("root"));
    }

    // Common to all
    configuration.password = settings.get_string("password").unwrap_or(String::new());
    configuration.path = settings.get_string("path").unwrap_or(String::from("./migrations"));
    configuration.migration_type = settings.get_string("migration_type").unwrap_or(String::from("migration"));

    configuration
}

/// Extract application parameters submitted by user.
///
/// # Arguments
///
/// * `cmd` - Type of command (down or up)
/// * `args` - Program args.
fn extract_parameters(cmd: &str, args: &ArgMatches) -> Configuration {
    let file_configuration = read_config_file(args);

    let mut configuration = Configuration {
        command: CommandName::UP,
        url: args.get_one::<String>("url").unwrap_or(&String::from("")).to_string(),
        engine: file_configuration.engine,
        host: args.get_one::<String>("host").unwrap_or(&String::from(file_configuration.host)).to_string(),
        port: *args.get_one::<u32>("port").unwrap_or(&file_configuration.port),
        database: args.get_one::<String>("database").unwrap_or(&String::from(file_configuration.database)).to_string(),
        username: args.get_one::<String>("username").unwrap_or(&String::from(file_configuration.username)).to_string(),
        password: file_configuration.password,
        table: args.get_one::<String>("migration_table").unwrap_or(&String::from(file_configuration.table)).to_string(),
        path: args.get_one::<String>("path").unwrap_or(&String::from(file_configuration.path)).to_string(),
        interactive: args.get_flag("interactive"),
        continue_on_error: args.get_flag("continue-on-error"),
        version: args.get_one::<String>("version").unwrap_or(&String::from("")).to_string(),
        migration_type: file_configuration.migration_type,
        step: 0,
        debug: args.get_flag("debug"),
        skip_transactions: args.get_flag("skip-transactions"),
        interactive_days: 0,
        create_name: args.get_one::<String>("name").unwrap_or(&String::from("")).to_string(),
        create_type: CreateType::FOLDER,
    };

    if args.get_flag("engine") {
        let default_engine = String::from("postgresql");
        let engine = args.get_one::<String>("engine").unwrap_or(&default_engine).as_str();
        configuration.engine = match engine {
            "mysql" => EngineName::MYSQL,
            "sqlite" => EngineName::SQLITE,
            _ => EngineName::POSTGRESQL
        };
    }

    if args.get_flag("password") {
        let term = Term::stdout();
        write!(&term, "Password:").unwrap();
        let password = term.read_secure_line().unwrap();
        configuration.password = password;
    }

    // Specific to interactive command
    if cmd == "interactive" || cmd == "status" {
        configuration.command = if cmd == "interactive" {
            CommandName::INTERACTIVE
        } else {
            CommandName::STATUS
        };

        configuration.interactive_days = if args.get_flag("days") {
            args.get_one::<String>("days").unwrap_or(&String::from("0")).parse::<u32>().unwrap_or(0)
        } else if args.get_flag("last-month") {
            31
        } else {
            0
        };
    }

    // Specific to up command
    if cmd == "up" {
        configuration.step = args.get_one::<String>("step").unwrap_or(&String::from("0")).parse::<u32>().unwrap_or(0);
    }

    // Specific to down command
    if cmd == "down" {
        configuration.command = CommandName::DOWN;
        configuration.step = if args.get_flag("all") {
            0
        } else {
            // Default, if nothing is set, will be 1.
            args.get_one::<String>("step").unwrap_or(&String::from("1")).parse::<u32>().unwrap_or(1)
        };
    }

    // Specific to create command
    if cmd == "create" {
        configuration.command = CommandName::CREATE;
        let default_create_type = String::from("folder");
        let create_type = args.get_one::<String>("folder_type").unwrap_or(&default_create_type).as_str();
        configuration.create_type = match create_type {
            "file" | "files" => CreateType::FILE,
            "split" | "split-file" | "split-files" => CreateType::SPLITFILES,
            _ => CreateType::FOLDER
        };
    }

    // Url override everything
    if configuration.url.len() > 0 {
        configuration.engine = if configuration.url.starts_with("mysql") == true {
            EngineName::MYSQL
        } else if configuration.url.starts_with("postgres") == true || configuration.url.contains("host=") == true {
            EngineName::POSTGRESQL
        } else {
            EngineName::SQLITE
        };
    }

    configuration
}

/// Run the migration
///
/// # Arguments
///
/// * `configuration` - Configuration of the application
fn apply_command(configuration: &Configuration) -> bool {
    match configuration.command {
        CommandName::CREATE => create::process(configuration),
        CommandName::UP => up::process(configuration),
        CommandName::DOWN => down::process(configuration),
        CommandName::INTERACTIVE => interactive::process(configuration),
        CommandName::STATUS => status::process(configuration),
    }
}



fn main() {
    // Compute the whole time to parse & do everything
    let whole_application_time = Instant::now();

    // Logger
    // Logging to stdout if below or equal to warning level
    let decorator_stdout = slog_term::TermDecorator::new().stdout().build();
    let drain_stdout = slog_term::CompactFormat::new(decorator_stdout).use_custom_timestamp(timestamp_utc).build().fuse();
    let drain_stdout = drain_stdout.filter(|r| r.level().as_usize() >= slog::Level::Warning.as_usize()).fuse();
    let drain_stdout = slog_async::Async::new(drain_stdout).build().fuse();
    // Logging to stderr if above warning level or below
    let decorator_stderr = slog_term::TermDecorator::new().stderr().build();
    let drain_stderr = slog_term::CompactFormat::new(decorator_stderr).use_custom_timestamp(timestamp_utc).build().fuse();
    let drain_stderr = drain_stderr.filter(|r| r.level().as_usize() < slog::Level::Warning.as_usize()).fuse();
    let drain_stderr = slog_async::Async::new(drain_stderr).build().fuse();
    // Building logger
    let drain_both = slog::Duplicate(drain_stdout, drain_stderr);
    let guard = slog_scope::set_global_logger(slog::Logger::root(drain_both.fuse(), o!()));

    // Command line arguments & parsing
    let base = Command::new("base")
        .about("base")
        .arg(Arg::new("url")
            .short('u')
            .long("url")
            .value_name("URL")
            .help("Set the url used to connect to database")
            .conflicts_with_all(&["config", "engine", "host", "port", "database", "username", "password"]))
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .value_name("FILE")
            .help("Load config file [default: migration.(json|hjson|yml|toml)]")
            .conflicts_with("url"))
        .arg(Arg::new("engine")
            .short('e')
            .long("engine")
            .value_name("ENGINE")
            .help("Define which engine [default: postgresql]")
            .conflicts_with("url"))
        .arg(Arg::new("host")
            .short('h')
            .long("host")
            .value_name("HOST")
            .help("Set the database host [default: 127.0.0.1]")
            .conflicts_with("url"))
        .arg(Arg::new("port")
            .short('p')
            .long("port")
            .value_name("PORT")
            .help("Set the database port [default: 6379 (postgres) | 3306 (mysql)]")
            .conflicts_with("url"))
        .arg(Arg::new("database")
            .short('d')
            .long("database")
            .value_name("DATABASE")
            .help("Set the database name [default: postgres (postgres) | mysql (mysql)]")
            .conflicts_with("url"))
        .arg(Arg::new("username")
            .short('U')
            .long("username")
            .value_name("USERNAME")
            .help("Set the database username [default: postgres (postgres) | root (mysql)]")
            .conflicts_with("url"))
        .arg(Arg::new("password")
            .short('W')
            .long("password")
            .help("Set the database password")
            .conflicts_with("url")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("path")
            .long("path")
            .value_name("PATH")
            .help("Folder to locate migration scripts [default: ./migrations]"))
        .arg(Arg::new("migration_type")
            .long("migration_type")
            .value_name("MIGRATION_TYPE")
            .help("Set the type of migration [default: migration]"))
        .arg(Arg::new("debug")
            .long("debug")
            .help("If set, this parameter will only print the configuration and do nothing")
            .action(clap::ArgAction::SetTrue));

    // Create command
    let mut create = base.clone();
    create = create.name("create")
        .about("Create a new migration file")
        .arg(Arg::new("folder_type")
            .long("folder_type")
            .value_name("FOLDER_TYPE")
            .help("Create a folder containing up and down files [default: folder]"))
        .arg(Arg::new("name")
            .value_name("MIGRATION_NAME")
            .help("The migration's name"));

    // Up is a copy of base with the version...
    let mut up = base.clone();
    up = up.name("up")
        .about("migrate database")
        .arg(Arg::new("version")
            .long("version")
            .value_name("VERSION")
            .help("Take care of only one specific migration script (based on timestamp)")
            .conflicts_with("step"))
        .arg(Arg::new("migration_table")
            .long("migration_table")
            .short('t')
            .value_name("TABLE_NAME")
            .help("Set the default migration table name"))
        .arg(Arg::new("step")
            .long("step")
            .value_name("NUMBER_OF_STEP")
            .help("Rollback X step(s) from the last found in database")
            .conflicts_with("version"))
        .arg(Arg::new("skip-transactions")
            .long("skip-transactions")
            .help("If set, each file that has to be migrated WILL NOT run in a transaction, note that you can set this per file")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("continue-on-error")
            .long("continue-on-error")
            .help("Continue if an error is encoutered (not recommended)")
            .action(clap::ArgAction::SetTrue));

    // Interactive also supports version but it's a different thing...
    let mut interactive = base.clone();
    interactive = interactive.name("interactive")
        .about("migrate up/down in an easy way")
        .arg(Arg::new("version")
            .long("version")
            .value_name("VERSION")
            .help("Start from the given version (don't care of previous ones)"))
        .arg(Arg::new("migration_table")
            .long("migration_table")
            .short('t')
            .value_name("TABLE_NAME")
            .help("Set the default migration table name"))
        .arg(Arg::new("days")
            .long("days")
            .value_name("NUMBER_OF_DAYS")
            .help("How many days back we should use for the interactive mode (any migration before X days will not be shown)"))
        .arg(Arg::new("last-month")
            .long("last-month")
            .help("Same as days except it automatically takes 31 days")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("skip-transactions")
            .long("skip-transactions")
            .help("If set, each file that has to be migrated WILL NOT run in a transaction, note that you can set this per file")
            .action(clap::ArgAction::SetTrue));

    let mut status = interactive.clone();
    status = status.name("status")
        .about("check the database status regarding migrations");

    let _custom_interactive = interactive.clone();

    // Down is a copy of up with the step...
    let mut down = base.clone();
    down = down.name("down")
           .about("rollback database")
        .arg(Arg::new("version")
            .long("version")
            .value_name("VERSION")
            .help("Take care of only one specific migration script (based on timestamp)")
            .conflicts_with("step"))
        .arg(Arg::new("skip-transactions")
            .long("skip-transactions")
            .help("If set, each file that has to be migrated WILL NOT run in a transaction, note that you can set this per file")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("continue-on-error")
            .long("continue-on-error")
            .help("Continue if an error is encoutered (not recommended)")
            .action(clap::ArgAction::SetTrue))
        .arg(Arg::new("migration_table")
            .long("migration_table")
            .short('t')
            .value_name("TABLE_NAME")
            .help("Set the default migration table name"))
        .arg(Arg::new("step")
            .long("step")
            .value_name("NUMBER_OF_STEP")
            .help("Rollback X step(s) from the last found in database")
            .conflicts_with("version"))
        .arg(Arg::new("all")
            .long("all")
            .help("If set, will rollback everything (dangerous)")
            .action(clap::ArgAction::SetTrue));

    let matches = Command::new("Migration")
        .version("0.1.5")
        .about("Handle migration of database schema")
        .subcommand(create)
        .subcommand(up)
        .subcommand(down)
        .subcommand(interactive)
        .subcommand(status)
        .get_matches();

    // Selecting the right sub-command to run
    let configuration: Configuration = match matches.subcommand() {
        Some(("create", create_matches)) => extract_parameters("create", &create_matches),
        Some(("up", up_matches)) => extract_parameters("up", &up_matches),
        Some(("down", down_matches)) => extract_parameters("down", &down_matches),
        Some(("status", status_matches)) => extract_parameters("status", &status_matches),
        Some(("", interactive_options)) | Some(("interactive", interactive_options)) => {
            extract_parameters("interactive", &interactive_options)
        },
        /*
        Some(("", _)) | Some(("interactive", _))  => {
            // We generate some fake pre-defined command args
            let custom_matches = Command::new("Migration")
                .subcommand(custom_interactive)
                .get_matches_from(vec!["migrate", "interactive", "-c", "migration"]);

            match custom_matches.subcommand() {
                Some(("interactive", interactive_matches)) => extract_parameters("interactive", &interactive_matches),
                Some(("", _)) => {
                    info!("Use --help to get started with");
                    Default::default()
                },
                _ => unreachable!(),
            }
        },
        */
        _ => unreachable!(), // If all sub-commands are defined above, anything else is unreachable!()
    };

    // Starting the application
    let result = apply_command(&configuration);
    let time_taken = &helpers::readable_time(whole_application_time.elapsed().as_millis());

    match result {
        true => debug!("done, took {}", time_taken),
        false => {
            crit!("failed, took {}", time_taken);
            drop(guard);
            std::process::exit(1);
        },
    }
}
