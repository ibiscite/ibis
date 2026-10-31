/// UTC source-added metadata. Missing or invalid dates never become a
/// publication year or an invented import time in Recently added.
pub fn added_date(raw: &str) -> Option<String> {
    let value = raw.trim().replace(' ', "T");
    let value = match value.len() { 10 => format!("{value}T00:00:00Z"), 19 => format!("{value}Z"), 20 => value, _ => return None };
    let b = value.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' { return None; }
    let number = |start: usize, end: usize| value.get(start..end)?.parse::<u32>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let days = match month { 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, 4 | 6 | 9 | 11 => 30, 2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29, 2 => 28, _ => return None };
    (year > 0 && day > 0 && day <= days && number(11, 13)? < 24 && number(14, 16)? < 60 && number(17, 19)? < 60).then_some(value)
}
