/// Parses a due date shared by `issue create` and `issue update` without touching the network.
pub(super) fn parse_due_date(value: &str) -> Result<String, &'static str> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return Err("due-date must use YYYY-MM-DD");
    }
    let year = value[..4].parse::<u16>().map_err(|_| "invalid due-date")?;
    let month = value[5..7].parse::<u8>().map_err(|_| "invalid due-date")?;
    let day = value[8..10].parse::<u8>().map_err(|_| "invalid due-date")?;
    let leap = year != 0 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err("due-date must be a valid calendar date"),
    };
    if year == 0 || !(1..=max_day).contains(&day) {
        return Err("due-date must be a valid calendar date");
    }
    Ok(value.to_owned())
}
