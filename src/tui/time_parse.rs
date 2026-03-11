use chrono::{DateTime, Datelike, Local, NaiveTime, TimeZone, Timelike, Utc, Weekday};

/// Parse a natural language time string into a UTC DateTime.
/// Returns None if the input cannot be parsed.
///
/// Supported formats:
///   "9am", "9:00", "21:30"                → today (or tomorrow if past)
///   "tomorrow 9am", "tomorrow 14:30"      → tomorrow at that time
///   "monday 3pm", "fri 9:00"              → next occurrence of weekday
///   "Mar 15 9am", "mar 15 14:30"          → specific month + day + time (current year)
///   "2026-03-15 09:00"                    → ISO-ish date + time
pub fn parse_schedule_time(input: &str) -> Option<DateTime<Utc>> {
    let input = input.trim().to_lowercase();
    if input.is_empty() {
        return None;
    }

    let parts: Vec<&str> = input.split_whitespace().collect();

    match parts.len() {
        // Single token: just a time like "9am", "14:30"
        1 => {
            let time = parse_time(parts[0])?;
            let today = Local::now().date_naive();
            let dt = today.and_time(time);
            let local_dt = Local.from_local_datetime(&dt).single()?;
            // If already past, roll to tomorrow
            if local_dt <= Local::now() {
                let tomorrow = today.succ_opt()?;
                let dt = tomorrow.and_time(time);
                let local_dt = Local.from_local_datetime(&dt).single()?;
                Some(local_dt.with_timezone(&Utc))
            } else {
                Some(local_dt.with_timezone(&Utc))
            }
        }
        // Two tokens: "tomorrow 9am", "monday 3pm", "2026-03-15 09:00"
        2 => {
            // Try ISO-ish: "2026-03-15 09:00"
            if let Some(dt) = try_parse_iso(&parts[0], &parts[1]) {
                return Some(dt);
            }
            // Try "tomorrow <time>"
            if parts[0] == "tomorrow" {
                let time = parse_time(parts[1])?;
                let tomorrow = Local::now().date_naive().succ_opt()?;
                let dt = tomorrow.and_time(time);
                let local_dt = Local.from_local_datetime(&dt).single()?;
                return Some(local_dt.with_timezone(&Utc));
            }
            // Try "<weekday> <time>"
            if let Some(weekday) = parse_weekday(parts[0]) {
                let time = parse_time(parts[1])?;
                let date = next_weekday(weekday);
                let dt = date.and_time(time);
                let local_dt = Local.from_local_datetime(&dt).single()?;
                return Some(local_dt.with_timezone(&Utc));
            }
            None
        }
        // Three tokens: "Mar 15 9am"
        3 => {
            let month = parse_month(parts[0])?;
            let day: u32 = parts[1].trim_end_matches(',').parse().ok()?;
            let time = parse_time(parts[2])?;
            let now = Local::now();
            let year = now.year();
            let date = chrono::NaiveDate::from_ymd_opt(year, month, day)?;
            let dt = date.and_time(time);
            let local_dt = Local.from_local_datetime(&dt).single()?;
            // If the date is in the past, advance to next year
            if local_dt <= now {
                let date = chrono::NaiveDate::from_ymd_opt(year + 1, month, day)?;
                let dt = date.and_time(time);
                let local_dt = Local.from_local_datetime(&dt).single()?;
                Some(local_dt.with_timezone(&Utc))
            } else {
                Some(local_dt.with_timezone(&Utc))
            }
        }
        _ => None,
    }
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    // Try "9:00", "14:30", "9:30am", "9:30pm" — check colon first to avoid
    // strip_suffix("am") consuming "9:30am" and failing to parse "9:30" as int.
    if s.contains(':') {
        let clean = s.trim_end_matches(|c: char| c.is_alphabetic());
        let suffix = &s[clean.len()..];
        let parts: Vec<&str> = clean.split(':').collect();
        if parts.len() == 2 {
            let mut hour: u32 = parts[0].parse().ok()?;
            let min: u32 = parts[1].parse().ok()?;
            if suffix == "pm" && hour != 12 {
                hour += 12;
            } else if suffix == "am" && hour == 12 {
                hour = 0;
            }
            return NaiveTime::from_hms_opt(hour, min, 0);
        }
    }
    // Try "9am", "9pm", "11am", "11pm"
    if let Some(rest) = s.strip_suffix("am") {
        let hour: u32 = rest.parse().ok()?;
        let hour = if hour == 12 { 0 } else { hour };
        return NaiveTime::from_hms_opt(hour, 0, 0);
    }
    if let Some(rest) = s.strip_suffix("pm") {
        let hour: u32 = rest.parse().ok()?;
        let hour = if hour == 12 { 12 } else { hour + 12 };
        return NaiveTime::from_hms_opt(hour, 0, 0);
    }
    None
}

fn parse_weekday(s: &str) -> Option<Weekday> {
    match s {
        "monday" | "mon" => Some(Weekday::Mon),
        "tuesday" | "tue" | "tues" => Some(Weekday::Tue),
        "wednesday" | "wed" => Some(Weekday::Wed),
        "thursday" | "thu" | "thur" | "thurs" => Some(Weekday::Thu),
        "friday" | "fri" => Some(Weekday::Fri),
        "saturday" | "sat" => Some(Weekday::Sat),
        "sunday" | "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

fn next_weekday(target: Weekday) -> chrono::NaiveDate {
    let today = Local::now().date_naive();
    let today_weekday = today.weekday();
    let days_ahead =
        (target.num_days_from_monday() as i64 - today_weekday.num_days_from_monday() as i64 + 7)
            % 7;
    // If today is the target weekday, schedule for next week
    let days_ahead = if days_ahead == 0 { 7 } else { days_ahead };
    today + chrono::Duration::days(days_ahead)
}

fn parse_month(s: &str) -> Option<u32> {
    match s {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

fn try_parse_iso(date_str: &str, time_str: &str) -> Option<DateTime<Utc>> {
    let date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    let time = parse_time(time_str)?;
    let dt = date.and_time(time);
    let local_dt = Local.from_local_datetime(&dt).single()?;
    let utc_dt = local_dt.with_timezone(&Utc);
    // Reject past dates — scheduling in the past makes no sense
    if utc_dt <= Utc::now() {
        return None;
    }
    Some(utc_dt)
}

/// Format a UTC DateTime for display in local time.
pub fn format_local_time(dt: &DateTime<Utc>) -> String {
    let local = dt.with_timezone(&Local);
    local.format("%b %d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_time_12h() {
        assert_eq!(parse_time("9am"), NaiveTime::from_hms_opt(9, 0, 0));
        assert_eq!(parse_time("3pm"), NaiveTime::from_hms_opt(15, 0, 0));
        assert_eq!(parse_time("12am"), NaiveTime::from_hms_opt(0, 0, 0));
        assert_eq!(parse_time("12pm"), NaiveTime::from_hms_opt(12, 0, 0));
    }

    #[test]
    fn parse_time_24h() {
        assert_eq!(parse_time("9:00"), NaiveTime::from_hms_opt(9, 0, 0));
        assert_eq!(parse_time("14:30"), NaiveTime::from_hms_opt(14, 30, 0));
        assert_eq!(parse_time("0:00"), NaiveTime::from_hms_opt(0, 0, 0));
    }

    #[test]
    fn parse_time_mixed() {
        assert_eq!(parse_time("9:30am"), NaiveTime::from_hms_opt(9, 30, 0));
        assert_eq!(parse_time("9:30pm"), NaiveTime::from_hms_opt(21, 30, 0));
    }

    #[test]
    fn parse_weekday_full_and_abbrev() {
        assert_eq!(parse_weekday("monday"), Some(Weekday::Mon));
        assert_eq!(parse_weekday("fri"), Some(Weekday::Fri));
        assert_eq!(parse_weekday("thurs"), Some(Weekday::Thu));
        assert_eq!(parse_weekday("xyz"), None);
    }

    #[test]
    fn parse_month_names() {
        assert_eq!(parse_month("jan"), Some(1));
        assert_eq!(parse_month("december"), Some(12));
        assert_eq!(parse_month("xyz"), None);
    }

    #[test]
    fn parse_schedule_empty_returns_none() {
        assert!(parse_schedule_time("").is_none());
        assert!(parse_schedule_time("   ").is_none());
    }

    #[test]
    fn parse_schedule_garbage_returns_none() {
        assert!(parse_schedule_time("asdfghjkl").is_none());
        assert!(parse_schedule_time("not a time").is_none());
    }

    #[test]
    fn parse_schedule_tomorrow() {
        let result = parse_schedule_time("tomorrow 9am").unwrap();
        let expected_date = (Local::now() + chrono::Duration::days(1)).date_naive();
        assert_eq!(result.with_timezone(&Local).date_naive(), expected_date);
    }

    #[test]
    fn parse_schedule_iso() {
        let result = parse_schedule_time("2026-12-25 14:30").unwrap();
        let local = result.with_timezone(&Local);
        assert_eq!(local.hour(), 14);
        assert_eq!(local.minute(), 30);
    }

    #[test]
    fn parse_schedule_iso_past_returns_none() {
        // A date in the past should return None
        assert!(parse_schedule_time("2020-01-01 09:00").is_none());
    }

    #[test]
    fn parse_schedule_month_day_past_rolls_to_next_year() {
        // "jan 1 9am" on March 11 2026 should schedule for Jan 1 2027
        let result = parse_schedule_time("jan 1 9am").unwrap();
        // The result should be in the future
        assert!(result > Utc::now());
    }

    #[test]
    fn parse_schedule_month_day_time() {
        let result = parse_schedule_time("mar 15 9am").unwrap();
        let local = result.with_timezone(&Local);
        assert_eq!(local.month(), 3);
        assert_eq!(local.day(), 15);
    }

    #[test]
    fn parse_schedule_weekday() {
        // Pick a weekday that is NOT today so the result is deterministic
        let today = Local::now().date_naive();
        let target_weekday = today.weekday().succ();
        let day_name = match target_weekday {
            Weekday::Mon => "monday",
            Weekday::Tue => "tuesday",
            Weekday::Wed => "wednesday",
            Weekday::Thu => "thursday",
            Weekday::Fri => "friday",
            Weekday::Sat => "saturday",
            Weekday::Sun => "sunday",
        };
        let input = format!("{} 3pm", day_name);
        let result = parse_schedule_time(&input).unwrap();
        let local = result.with_timezone(&Local);
        assert_eq!(local.hour(), 15);
        assert_eq!(local.minute(), 0);
        // Should be within the next 7 days
        let days_diff = (local.date_naive() - today).num_days();
        assert!(days_diff >= 1 && days_diff <= 7);
    }

    #[test]
    fn next_weekday_skips_today() {
        let today = Local::now().date_naive();
        let target = today.weekday();
        let result = next_weekday(target);
        assert_eq!(result, today + chrono::Duration::days(7));
    }

    #[test]
    fn format_local_time_reasonable() {
        let dt = Utc::now();
        let formatted = format_local_time(&dt);
        // Should contain a 3-letter month and day
        assert!(formatted.len() >= 10);
    }
}
