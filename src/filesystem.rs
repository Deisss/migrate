use glob::glob;
use std::path::PathBuf;
use regex::{Regex, RegexBuilder};
use std::default::Default;
use std::fs;
use std::error::Error;
use std::cmp::Ordering;

#[derive(Debug, Default, Clone)]
pub struct File {
    pub number: u64,
    pub name: String,
    pub file_stem: String,
    pub origin: PathBuf,
    pub is_up: bool,
    pub is_down: bool
}

impl PartialOrd for File {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.number.partial_cmp(&other.number)
    }
}

impl PartialEq for File {
    fn eq(&self, other: &Self) -> bool {
        self.number == other.number
    }
}

/// Parse file and extract useful content from it.
/// A file is supposed to be either:
///   - 0012_migration_name.sql
///   - 20201403211247_migration_name.sql
///
/// # Arguments
///
/// * `filename` - The original PathBuf from glob
fn extract_useful_information_from_file_name(original: PathBuf) -> Option<File> {
    // Taking care of some potential problems
    if !original.is_file() {
        return None;
    }
    match original.extension() {
        Some(extension) => {
            if extension != "sql" {
                return None;
            }
        }
        None => return None
    }

    let mut file: File = Default::default();
    let mut file_stem = original.file_stem()?;
    file.file_stem = String::from(file_stem.to_str()?);
    file.is_up = true;
    file.is_down = true;

    if file.file_stem.ends_with("up") {
        file.is_down = false;
    } else if file.file_stem.ends_with("down") {
        file.is_up = false;
    }

    // We have to get the parent in this case...
    if file_stem == "up" || file_stem == "down" {
        let mut it = original.iter().rev();
        // The file
        it.next();
        // The folder
        let res = it.next();
        if res.is_none() {
            return None;
        }
        // Now we have the parent (that should contains the number/rest value
        // we are looking for)
        file_stem = res.unwrap();
    }

    let mut file_stem: String = String::from(file_stem.to_str()?);
    file.origin = original.to_owned();

    if file_stem.ends_with("up") {
        file_stem.truncate(file_stem.len() - 2);
    } else if file_stem.ends_with("down") {
        file_stem.truncate(file_stem.len() - 4);
    }

    let re = Regex::new(r"^(?P<number>\d+)(?P<rest>.*)").unwrap();
    let data = re.captures(&file_stem)?;

    file.number = data["number"].parse::<u64>().unwrap_or(0);
    file.name = String::from(&data["rest"])
        .replace("_", " ")
        .replace("-", " ")
        .replace(".", " ");
    file.name = file.name.trim().to_string();

    Some(file)
}


/// Get all migration scripts within folder
///
/// # Arguments
///
/// * `root` - Root folder.
/// * `filter` - Possible filter to send (will reject any file below given value - used by interactive mode).
pub fn migrations(root: &str, filter: Option<String>) -> Vec<File> {
    if root.len() == 0 {
        return Vec::new();
    }
    let mut test = String::from(root);
    let len = test.len();
    let last = &test[len - 1..];

    if last != "/" && last != "\\" {
        test.push_str("/");
    }
    test.push_str("**/*.sql");

    let result = glob(&test);

    let mut vector: Vec<File> = Vec::new();
    let restrict: u64;
    match filter {
        Some(s) => restrict = s.parse::<u64>().unwrap_or(0),
        _ => restrict = 0
    }

    match result {
        Ok(results) => {
            for entry in results {
                match entry {
                    Ok(path) => {
                        let filename = path.to_owned().into_os_string().into_string();
                        let tmp = extract_useful_information_from_file_name(path);

                        if tmp.is_some() {
                            let tmp = tmp.unwrap();
                            if restrict > 0 {
                                if tmp.number >= restrict {
                                    vector.push(tmp);
                                }
                            } else {
                                vector.push(tmp);
                            }
                        } else {
                            match filename {
                                Ok(s) => warn!("Failed to get file: {}", s),
                                _ => {}
                            }
                        }
                    }
                    Err(e) => warn!("Could not access the file: {}", e)
                }
            }
        },
        Err(e) => warn!("Error while reading migration folder: {}", e)
    }

    vector
}

/// Load a file and transform it into a transaction based one.
///
/// # Arguments
///
/// * `filename` - The file to get.
/// * `migration_type` - If it's down (0), or up (1).
pub fn get_sql(file: &File, migration_type: u8) -> Result<String, Box<dyn Error>> {
    let s = fs::read_to_string(&file.origin)?;
    // In this specific case the type is used.
    if file.is_up && file.is_down {
        let re_down = RegexBuilder::new(r" *-- *=+ *down *=+").case_insensitive(true).build()?;

        if migration_type == 0 {
            let pos_down = re_down.find(&s);

            if pos_down.is_some() {
                let pos_down = pos_down.unwrap();
                let position = pos_down.end();
                let l = s.len();
                let extracted = s[position..l].trim().to_string();
                return Ok(extracted);
            }
        } else if migration_type == 1 {
            let re_up = RegexBuilder::new(r" *-- *=+ *up *=+").case_insensitive(true).build()?;
            let pos_up = re_up.find(&s);

            // We've found something...
            if pos_up.is_some() {
                let pos_up = pos_up.unwrap();

                if pos_up.start() < pos_up.end() {
                    let pos_down = re_down.find_at(&s, pos_up.end());
        
                    if pos_down.is_some() {
                        let pos_down = pos_down.unwrap();
                        let extracted = s[pos_up.end()..pos_down.start()].trim().to_string();
                        return Ok(extracted);
                    }
                }
            }
        }

    }
    Ok(s)
}

/// Replace "\\" to "/" and remove "./" if any.
///
/// # Arguments
///
/// * `migration_folder` - The folder path to remove from file path.
/// * `migration_file` - The file to get printable content from.
fn uniform_path_str(s: &str) -> String {
    let mut s = String::from(s).replace("\\", "/");
    if s.starts_with("./") {
        s = String::from(&s[2..]);
    }
    s
}

/// Remove the migration folder from the file path.
///
/// # Arguments
///
/// * `migration_folder` - The folder path to remove from file path.
/// * `migration_file` - The file to get printable content from.
pub fn get_file_path_without_migration_path(migration_folder: &str, migration_file: &str) -> String {
    let folder = uniform_path_str(migration_folder);
    let file = uniform_path_str(migration_file);

    let folder_c: Vec<_> = folder.chars().collect();
    let size: i32 = file.len() as i32 - folder.len() as i32;

    let mut s = String::with_capacity(size.abs() as usize);
  
    for (i, c) in file.chars().enumerate() {
        let t = folder_c.get(i);
        if t.is_none() || t.unwrap() != &c {
            s.push(c);
        }
    }
    if s.starts_with("/") {
        return String::from(&s[1..]);
    }
    s
}
