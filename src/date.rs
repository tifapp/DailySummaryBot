use chrono::{Local, NaiveDate, format::ParseError, prelude::*};
use crate::tracing::info;

pub fn print_current_date() -> String {
    let now = Utc::now();
    now.format("%m/%d/%y").to_string()
}

pub fn days_until(future_MMDDYYY: &str) -> Result<i64, ParseError> {
    let now = Local::now().date_naive();
    info!("Current date: {:?}", now);
    let future_date = NaiveDate::parse_from_str(&future_MMDDYYY, "%m/%d/%Y")?;
    info!("Future date: {:?}", future_date);
    let difference = future_date.signed_duration_since(now).num_days();
    info!("Days difference: {:?}", difference);
    
    Ok(difference)
}