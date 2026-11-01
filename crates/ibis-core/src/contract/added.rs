/// Normalizes a source's added date to UTC `YYYY-MM-DDTHH:MM:SSZ`.
/// Accepts a date, a date and time (read as UTC, as Zotero writes it), and a
/// date and time with `Z` or a numeric offset such as BibDesk's `+0000` or
/// `-05:00`. Anything else, including impossible dates, returns `None`.
pub fn added_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let (local, offset) = match raw.len() {
        10 | 19 => (raw, 0),
        _ => (raw.get(..19)?, offset_minutes(raw.get(19..)?.trim_start())?),
    };
    let value = local.replace(' ', "T");
    let value = if value.len() == 10 {
        format!("{value}T00:00:00")
    } else {
        value
    };
    let b = value.as_bytes();
    if b.len() != 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    if b.iter()
        .enumerate()
        .any(|(index, byte)| !matches!(index, 4 | 7 | 10 | 13 | 16) && !byte.is_ascii_digit())
    {
        return None;
    }
    let number = |start: usize, end: usize| value.get(start..end)?.parse::<i64>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return None,
    };
    if year == 0 || day == 0 || day > days || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    // Shift to UTC through whole days, so month and year boundaries stay exact.
    let minutes = days_from_civil(year, month, day) * 1440 + hour * 60 + minute - offset;
    let (year, month, day) = civil_from_days(minutes.div_euclid(1440));
    let (hour, minute) = (minutes.rem_euclid(1440) / 60, minutes.rem_euclid(60));
    (1..=9999)
        .contains(&year)
        .then(|| format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"))
}

/// `Z`, `+HHMM`, `+HH:MM` or the same with `-`, as minutes east of UTC.
fn offset_minutes(tail: &str) -> Option<i64> {
    if tail == "Z" {
        return Some(0);
    }
    let sign = match tail.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hours, minutes) = match tail.get(1..)? {
        digits if digits.len() == 4 => (digits.get(..2)?, digits.get(2..)?),
        digits if digits.len() == 5 && digits.as_bytes()[2] == b':' => (digits.get(..2)?, digits.get(3..)?),
        _ => return None,
    };
    if !(hours.bytes().chain(minutes.bytes())).all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (hours, minutes) = (hours.parse::<i64>().ok()?, minutes.parse::<i64>().ok()?);
    (hours <= 14 && minutes <= 59).then_some(sign * (hours * 60 + minutes))
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    (era * 400 + year_of_era + i64::from(month <= 2), month, day)
}
