use chrono::{Local, NaiveDate, format::ParseError, prelude::*};

pub fn print_current_date() -> String {
    let now = Utc::now();
    now.format("%m/%d/%y").to_string()
}

pub fn days_between(mmddyyy_1: Option<&str>, mmddyyy_2: &str) -> Result<i64, ParseError> {
    let past_date = match mmddyyy_1 {
        Some(date_str) => NaiveDate::parse_from_str(date_str, "%m/%d/%Y")?,
        None => Local::today().naive_local(),
    };

    let future_date = NaiveDate::parse_from_str(mmddyyy_2, "%m/%d/%Y")?;

    let difference = future_date.signed_duration_since(past_date).num_days();

    Ok(difference)
}