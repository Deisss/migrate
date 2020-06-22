use crate::filesystem;
use crate::Configuration;
use crate::EngineName;
use crate::engines::get_sql_engine;
use crate::filesystem::{File, get_sql, get_file_path_without_migration_path};
use crate::commands::up::process_up_sql;
use crate::commands::down::process_down_sql;
use crate::helpers::{limit_number, limit_per_date};
use super::debug_configuration;
use console::{Style, Term, Key};
use std::error::Error;
use std::default::Default;
use std::cmp::Ordering;
use std::io::{stdin, stdout, Write};
use std::thread;
use std::time::Duration;

#[derive(Clone, PartialEq)]
pub enum InteractionType {
    NONE,
    DOWN,
    UP,
    REDO
}

impl Default for InteractionType {
    fn default() -> Self { InteractionType::NONE }
}

#[derive(Clone, Default)]
pub struct InteractiveMigration {
    pub current_type: InteractionType,
    pub new_type: InteractionType,
    pub number: String,
    pub file_up: Option<File>,
    pub file_down: Option<File>,
    pub migration: Option<String>,
    pub migration_hash: Option<String>,
    pub migration_origin: Option<String>,
    pub file_up_hash: Option<String>,
}

impl PartialOrd for InteractiveMigration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.number.partial_cmp(&other.number)
    }
}

impl PartialEq for InteractiveMigration {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number
    }
}

/// Transform a migration into an interactive migration.
///
/// # Arguments
///
/// * `migration` - The migration to transform.
/// * `hash` - The md5 hash of the original migrated file.
/// * `origin` - The file origin (it's file path) -used in case of missing file-.
fn convert_migration_to_interactive(migration: &str, hash: &str, origin: &str) -> InteractiveMigration {
    let mut result: InteractiveMigration = Default::default();
    result.current_type = InteractionType::UP;
    result.number = String::from(migration);
    result.migration = Some(String::from(migration));
    result.migration_hash = Some(String::from(hash));
    result.migration_origin = Some(String::from(origin));
    result.file_up = None;
    result.file_down = None;
    result
}

/// Transform a file into an interactive migration.
///
/// # Arguments
///
/// * `file` - The file to transform.
fn convert_file_to_interactive(file: &File) -> InteractiveMigration {
    let mut result: InteractiveMigration = Default::default();
    let tmp = file.clone();
    let s = tmp.number.to_string();
    result.current_type = InteractionType::DOWN;
    result.migration = None;
    result.migration_hash = None;
    result.migration_origin = None;
    result.number = s;
    result.file_up = Some(tmp);
    result.file_up_hash = None;
    result.file_down = None;
    result
}

/// Create the needed interactive array.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
/// * `files` - The files.
pub fn merge_migrations_and_files(migrations: &Vec<(String, String, String)>, files: &Vec<File>) -> Vec<InteractiveMigration> {
    let mut results: Vec<InteractiveMigration> = Vec::with_capacity(migrations.len());
    for migration in migrations {
        results.push(convert_migration_to_interactive(&migration.0, &migration.1, &migration.2));
    }

    // First we make sure the array is complete, any UP is registered.
    for file in files {
        let mut found = false;
        for migration in results.iter_mut() {
            if migration.number == file.number.to_string() {
                found = true;
                break;
            }
        }
        if found == false && file.is_up == true {
            results.push(convert_file_to_interactive(&file));
        }
    }

    // The we associate all of them to the related down/up file.
    for file in files {
        for migration in results.iter_mut() {
            if migration.number == file.number.to_string() {
                // We can't do an else here as a file
                // can be both up and down...
                if file.is_down == true {
                    migration.file_down = Some(file.clone());
                }
                if file.is_up == true {
                    let c = file.clone();
                    match get_sql(&c, 1) {
                        Ok(sql) => {
                            let hash = format!("{:x}", md5::compute(&sql));
                            migration.file_up = Some(c);
                            migration.file_up_hash = Some(hash);
                        },
                        Err(e) => {
                            warn!("{} failed to read: {}", c.origin.display(), e);
                            migration.file_up = Some(c);
                        }
                    }
                }
            }
        }
    }

    // We sort and return
    results.sort_by(|f1, f2| f1.partial_cmp(f2).unwrap());
    results
}

/// Show the content of the menu (specific to this migration system).
///
/// # Arguments
///
/// * `term` - The terminal object.
/// * `root` - The folder where migrations are.
/// * `migrations` - The elements to show.
/// * `selected` - The selected position.
fn print_menu(term: &Term, root: &str, migrations: &Vec<InteractiveMigration>, selected: usize) -> std::io::Result<Vec<usize>> {
    let installed = Style::new().green();
    let not_installed = Style::new().red();
    let cyan = Style::new().cyan();
    let yellow = Style::new().yellow();
    let inactive = Style::new().dim();
    let mut results: Vec<usize> = Vec::with_capacity(migrations.len());

    // need to specify number of lines here
    let r = term.write_line("");
    if r.is_err() {
        crit!("Terminal error: {:?}", r.err());
    }
    let r = term.write_line("   Installed |   To Do   | migration number | name");
    if r.is_err() {
        crit!("Terminal error: {:?}", r.err());
    }
    let r = term.write_line("  -----------+-----------+------------------+-----------------");
    if r.is_err() {
        crit!("Terminal error: {:?}", r.err());
    }
    results.push(0);
    results.push(50);
    results.push(62);

    for index in 0..migrations.len() {
        if let Some(migration) = migrations.get(index) {
            let mut content = String::new();
            // We have to count not linked to the string as the string
            // includes a lots of unseen characters (for color)
            let mut size: usize = 0;

            if selected == index {
                content.push_str(&format!("{} ", cyan.apply_to(">")));
            } else {
                content.push_str("  ");
            }
            size += 2;
    
            if migration.current_type == InteractionType::UP {
                let m_hash = migration.migration_hash.as_ref();
                let f_hash = migration.file_up_hash.as_ref();
                if m_hash.is_some() && f_hash.is_some() && Some(m_hash) == Some(f_hash) {
                    content.push_str(&format!("    {}    ", installed.apply_to("yes")));
                } else if f_hash.is_some() {
                    content.push_str(&format!("  {}  ", yellow.apply_to("changed")));
                } else {
                    content.push_str(&format!("  {}  ", yellow.apply_to("missing")));
                }
                
            } else {
                content.push_str(&format!("    {}     ", not_installed.apply_to("no")));
            }
            size += 11;
    
            match migration.new_type {
                InteractionType::NONE => content.push_str("|           |"),
                InteractionType::UP =>   content.push_str(&format!("|  {}  |", cyan.apply_to("install"))),
                InteractionType::DOWN => content.push_str(&format!("| {} |", cyan.apply_to("uninstall"))),
                InteractionType::REDO => content.push_str(&format!("| {} |", cyan.apply_to("reinstall"))),
            }
            size += 13;
    
            content.push_str(" ");
            if selected == index {
                content.push_str(&limit_number(&migration.number));
            } else {
                content.push_str(&inactive.apply_to(&limit_number(&migration.number)).to_string());
            }
            content.push_str(" | ");
            // The number has a fixed size of 16 characters + 4 above and below
            size += 20;
    
            if migration.file_up.is_some() {
                let f = migration.file_up.as_ref().unwrap();
                let file_name = get_file_path_without_migration_path(root, &f.origin.display().to_string());
                if selected == index {
                    content.push_str(&format!("{} ({})", &f.name.to_owned(), file_name.to_owned()));
                } else {
                    content.push_str(&format!("{} {}{}{}", inactive.apply_to(&f.name.to_owned()),
                        inactive.apply_to("("), inactive.apply_to(file_name.to_owned()),
                        inactive.apply_to(")")
                    ));
                }
                size += 3 + f.name.len() + file_name.len();
            } else if migration.migration_origin.is_some() {
                if selected == index {
                    content.push_str(&format!("{} (was: {})", yellow.apply_to("missing file"), &migration.migration_origin.as_ref().unwrap()));
                } else {
                    content.push_str(&format!("{} {}{} {}{}", yellow.apply_to("missing file"),
                    inactive.apply_to("("), inactive.apply_to("was:"), inactive.apply_to(migration.migration_origin.as_ref().unwrap()),
                        inactive.apply_to(")")
                    ));
                }
                size += 20 + migration.migration_origin.as_ref().unwrap().len();
            }
            // content = content.replace("\"", "");
            term.write_line(&content.clone())?;
            results.push(size);
        }
    }

    if selected == migrations.len() {
        let s: String = format!("{} Apply", cyan.apply_to(">"));
        term.write_line(&s.clone())?;
    } else {
        let s: String = format!("  {}", inactive.apply_to("Apply"));
        term.write_line(&s.clone())?;
    }
    results.push(7);

    if selected == migrations.len() + 1 {
        let s: String = format!("{} Exit", cyan.apply_to(">"));
        term.write_line(&s.clone())?;
    } else {
        let s: String = format!("  {}", inactive.apply_to("Exit"));
        term.write_line(&s.clone())?;
    }
    results.push(6);

    Ok(results)
}

/// Clear the menu
///
/// # Arguments
///
/// * `term` - The terminal object.
/// * `sizes` - List of written lines so far.
fn clear_menu(term: &Term, sizes: &mut Vec<usize>) -> std::io::Result<()> {
    // First we need to get the size of the terminal
    let (_height, width) = term.size();
    let width: usize = width as usize;
    let mut nb_lines_to_clear: usize = 0;
    for original_size in sizes.iter() {
        let mut line_size = *original_size;
        while line_size > width && line_size > 0 {
            line_size -= width;
            nb_lines_to_clear += 1;
        }
        nb_lines_to_clear += 1;
    }
    term.clear_last_lines(nb_lines_to_clear)
}

/// Generate the interactive menu.
///
/// # Arguments
///
/// * `root` - The root of migration folder.
/// * `migrations` - The files to show.
fn show_interactive_menu(root: &str, migrations: &mut Vec<InteractiveMigration>) -> bool {
    let term = Term::stdout();
    let mut position: usize = 0;
    let mut rerender = false;


    let r = print_menu(&term, root, &migrations, position);
    if r.is_err() {
        crit!("Terminal error: {:?}", r.as_ref().err());
    }
    let mut rendered_sizes: Vec<usize> = r.unwrap();

    loop {
        if rerender == true {
            rerender = false;
            let r = clear_menu(&term, &mut rendered_sizes);
            if r.is_err() {
                crit!("Terminal error: {:?}", r.err());
            }
            let r = print_menu(&term, root, &migrations, position);
            if r.is_err() {
                crit!("Terminal error: {:?}", r.as_ref().err());
            }
            rendered_sizes = r.unwrap();
        }
        thread::sleep(Duration::from_millis(10));
        let res = term.read_key().unwrap();

        match res {
            Key::Enter | Key::Char(' ') => {
                if position < migrations.len() {
                    if let Some(current) = migrations.get_mut(position) {
                        if current.migration.is_some() {
                            if current.new_type == InteractionType::NONE {
                                current.new_type = InteractionType::DOWN;
                            } else if current.new_type == InteractionType::DOWN {
                                current.new_type = InteractionType::REDO;
                            } else {
                                current.new_type = InteractionType::NONE;
                            }
                        } else {
                            if current.new_type == InteractionType::UP {
                                current.new_type = InteractionType::NONE;
                            } else {
                                current.new_type = InteractionType::UP;
                            }
                        }
                        rerender = true;
                    }
                } else if position == migrations.len() {
                    // Return true when we want to exit with apply
                    let r = clear_menu(&term, &mut rendered_sizes);
                    if r.is_err() {
                        crit!("Terminal error: {:?}", r.err());
                    }
                    return true;
                } else if position == migrations.len() + 1 {
                    // Return false when we want to just quit
                    let r = clear_menu(&term, &mut rendered_sizes);
                    if r.is_err() {
                        crit!("Terminal error: {:?}", r.err());
                    }
                    return false;
                }
            },
            Key::ArrowUp => {
                if position > 0 {
                    position = position - 1;
                    rerender = true;
                }
            },
            Key::ArrowDown => {
                if position < migrations.len() + 1 {
                    position += 1;
                    rerender = true;
                }
            },
            _ => {}
        }
    }
}

/// Only print one type of type at a time.
///
/// # Arguments
///
/// * `name` - The name.
/// * `root` - The root path.
/// * `migrations` - The files to print.
/// * `type` - The type we want to print.
fn show_partial_recap_menu(name: &str, root: &str, migrations: &Vec<InteractiveMigration>, interaction: InteractionType) {
    let mut first = true;
    for migration in migrations {
        if migration.new_type == interaction || migration.new_type == InteractionType::REDO {
            if first == true {
                first = false;
                println!("{}", name);
                println!("--------------------");
            }
            let f = match interaction {
                InteractionType::UP => migration.file_up.as_ref().unwrap(),
                _ => migration.file_down.as_ref().unwrap(),
            };

            let file_name = get_file_path_without_migration_path(root, &f.origin.display().to_string());
            let s = format!("{}", file_name);
            println!("{}", s.replace("\"", ""));
        }
    }
    println!("");
}

/// Show what should be proceed.
///
/// # Arguments
///
/// * `root` - The root path.
/// * `migrations` - The files to migrate.
fn show_recap_menu(root: &str, migrations: &Vec<InteractiveMigration>) -> bool {
    show_partial_recap_menu("DOWN", root, migrations, InteractionType::DOWN);
    show_partial_recap_menu("UP", root, migrations, InteractionType::UP);
    print!("Sounds good [Y/n]:");
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

/// Do the interactive mode.
///
/// # Arguments
///
/// * `configuration` - The system configuration.
/// * `files` - The files.
fn process_interactive_sql(configuration: &Configuration, files: &mut Vec<File>) -> Result<(), Box<dyn Error>> {
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

    let existing = db.get_migrations_with_hashes(&configuration.migration_type);
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
    let we_have_to_migrate = show_interactive_menu(&configuration.path, &mut to_show);

    let mut we_have_migrations_to_do = false;
    for migration in to_show.iter() {
        if migration.new_type == InteractionType::UP || migration.new_type == InteractionType::DOWN || migration.new_type == InteractionType::REDO {
            we_have_migrations_to_do = true;
            break;
        }
    }

    if we_have_to_migrate && we_have_migrations_to_do {
        let confirm = show_recap_menu(&configuration.path, &to_show);
        if confirm {
            // First we do down + redo, in a reverse order
            let mut migration_up: Vec<File> = to_show.iter()
                .filter(|x| x.new_type == InteractionType::UP || x.new_type == InteractionType::REDO)
                .map(|x| x.file_up.as_ref().unwrap().clone()).collect();
            let mut migration_down: Vec<File> = to_show.iter()
                .filter(|x| x.new_type == InteractionType::DOWN || x.new_type == InteractionType::REDO)
                .map(|x| x.file_down.as_ref().unwrap().clone()).collect();

            // We make sure they are in the right order
            migration_up.sort_by(|f1, f2| f1.partial_cmp(f2).unwrap());
            migration_down.sort_by(|f1, f2| f2.partial_cmp(f1).unwrap());

            if migration_down.len() > 0 {
                debug!("REVERTING");
                debug!("");
                process_down_sql(configuration, &mut migration_down)?;
            }
            if migration_up.len() > 0 {
                debug!("MIGRATING");
                debug!("");
                process_up_sql(configuration, &mut migration_up)?;
            }
        }
    }

    Ok(())
}

/// Start the interactive mode...
///
/// # Arguments
///
/// * `configuration` - The configuration to use.
pub fn process(configuration: &Configuration) -> bool {
    // We have to exit right here
    if configuration.debug == true {
        debug_configuration(configuration, "", "", &Vec::new());
        return true;
    }

    let mut files = filesystem::migrations(&configuration.path, None);
    files.sort_by(|f1, f2| f1.partial_cmp(f2).unwrap());

    match configuration.engine {
        EngineName::POSTGRESQL | EngineName::SQLITE | EngineName::MYSQL => {
            match process_interactive_sql(configuration, &mut files) {
                Err(_e) => false,
                _ => true
            }
        }
    }
}