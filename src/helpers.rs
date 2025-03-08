use chrono::{Duration, Utc};
use crate::Configuration;
use regex::Regex;

/// Transform a time into a readable time.
///
/// # Arguments
///
/// * `time_ms` - The time in millisecond to get related string.
pub fn readable_time(time_ms: u128) -> String {
    let mut internal: u128 = time_ms;
    let mut result: String = String::new();

    let milliseconds: u128;
    let seconds: u128;
    let minutes: u128;
    let hours: u128;

    //3600000 milliseconds in an hour
    hours = internal / 3600000;
    internal = internal - 3600000 * hours;
    //60000 milliseconds in a minute
    minutes = internal / 60000;
    internal = internal - 60000 * minutes;
    //1000 milliseconds in a second
    seconds = internal / 1000;
    milliseconds = internal - 1000 * seconds;

    if hours > 0 {
        result.push_str(&hours.to_string());
        result.push_str("h ");
    }
    if minutes > 0 || hours > 0 {
        result.push_str(&minutes.to_string());
        result.push_str("min ");
    }
    if seconds > 0 || minutes > 0 || hours > 0 {
        result.push_str(&seconds.to_string());
        result.push_str("sec ");
    }
  
    result.push_str(&milliseconds.to_string());
    result.push_str("ms");

    result
}

/// Split a content line by line - without removing delimiter.
///
/// # Arguments
///
/// * `s` - The content to split.
fn split_new_line(s: &str) -> Vec<String> {
    let mut destructive = s;
    let mut lf = destructive.find('\n');
    let mut results: Vec<String> = Vec::new();

    while lf.is_some() {
        let (left, right) = destructive.split_at(lf.unwrap() + 1);
        results.push(String::from(left));
        destructive = right;
        lf = destructive.find('\n');
    }

    // We have to get the last part
    if destructive.len() > 0 {
        results.push(String::from(destructive));
    }
    results
}

/// Will extract the line that is relevant regarding given position.
///
/// # Arguments
///
/// * `content` - The content to extract.
/// * `position` - The position in the text we are searching for.
pub fn get_relevant_line(content: &str, position: u32) -> Option<(u32, u32, String)> {
    // We want to be sure the new line char is 1 char (simplify everything)...
    let mut cumulative: u32 = 0;
    let mut full_position: u32;
    let mut line_number: u32 = 0;
    for mut line in split_new_line(content) {
        line_number += 1;
        full_position = line.len() as u32;
        if cumulative + full_position > position {
            if line.ends_with("\r\n") {
                line.truncate(line.len() - 2);
            } else if line.ends_with("\n") || line.ends_with("\r") {
                line.truncate(line.len() - 1);
            }
            return Some((cumulative, line_number, line));
        }
        cumulative += full_position;
    }
    return None;
}

/// Compare a migration number and check if it's in range of today - nb days.
///
/// # Arguments
///
/// * `migration_number` - The migration number.
/// * `days` - The number of days.
pub fn limit_per_date(migration_number: &str, days: u32) -> bool {
    let dt = Utc::now() - Duration::days(days as i64);
    let n = dt.format("%Y%m%d%H%M%S").to_string().parse::<u64>().unwrap_or(0);
    let e = migration_number.parse::<u64>().unwrap_or(0);
    e > n
}

/// Fit a number into the given size allowed (16 chars).
///
/// # Arguments
///
/// * `number` - The number to fit.
pub fn limit_number(number: &str) -> String {
    if number.len() == 16 {
        number.to_string()
    } else if number.len() > 16 {
        // 13 because we count the "..."
        let space = number.len() - 13;
        let mut s = String::from("...");
        s.push_str(&number[space..]);
        s
    } else {
        let mut s = String::from(number);

        while s.len() < 16 {
            if s.len() % 2 == 0 {
                s.push(' ');
            } else {
                s.insert(0, ' ');
            }
        }

        s
    }

}

/// Fit a number into the given size allowed (16 chars).
///
/// # Arguments
///
/// * `configuration` - The current configuration.
/// * `sql` - The current migration file (can contains a specific skip transaction).
pub fn skip_transaction(configuration: &Configuration, sql: &str) -> bool {
    match configuration.skip_transactions {
        true => true,
        false => {
            let lines = sql.lines();
            let re = Regex::new(r"^--\s*migrate\s*:\s*no-transaction$").unwrap();

            // if any of the lines contains the following, we skip.
            for s in lines {
                if re.is_match(s) {
                    return true;
                }
            }

            false
        }
    }
}