/// Parsed drive info from a `DRV:` line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDrive {
    pub index: usize,
    pub visible: bool,
    pub enabled: bool,
    pub flags: u32,
    pub drive_name: String,
    pub disc_name: Option<String>,
}

/// Parsed progress from a `PRGV:` line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProgress {
    pub current: u64,
    pub total: u64,
    pub max: u64,
}

/// Parsed message from a `MSG:` line
#[derive(Debug, Clone)]
pub struct ParsedMessage {
    pub code: u32,
    pub flags: u32,
    pub message: String,
}

/// Unquote a makemkvcon robot-mode string (removes surrounding quotes,
/// unescapes `\"` → `"` and `\\` → `\`).
fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s);
    let s = s.strip_suffix('"').unwrap_or(s);
    s.replace("\\\"", "\"").replace("\\\\", "\\")
}

/// Split a DRV/MSG/etc line into the prefix and the rest.
/// Returns `None` if the prefix doesn't match.
fn split_prefix<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix)
}

/// Parse a comma-separated list of fields from a robot-mode line.
/// Handles quoted fields that may contain commas.
fn parse_csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in line.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut current));
            }
            '\\' if in_quotes => {
                current.push(ch);
            }
            _ => {
                current.push(ch);
            }
        }
    }
    fields.push(current);
    fields
}

/// Parse a `DRV:` line from `makemkvcon -r` output.
///
/// Format: `DRV:index,visible,enabled,flags,"drive_name","disc_name"`
///
/// # Example
/// ```ignore
/// let drive = parse_drive_line(r#"DRV:0,1,1,24,"HL-DT-ST BD-RE BH16NS40","MY_DISC""#);
/// assert_eq!(drive.unwrap().index, 0);
/// ```
pub fn parse_drive_line(line: &str) -> Option<ParsedDrive> {
    let rest = split_prefix(line, "DRV:")?;
    let fields = parse_csv_fields(rest);
    if fields.len() < 6 {
        return None;
    }

    let index = fields[0].parse().ok()?;
    let visible = fields[1].parse::<u8>().ok()? != 0;
    let enabled = fields[2].parse::<u8>().ok()? != 0;
    let flags = fields[3].parse().unwrap_or(0);
    let drive_name = unquote(&fields[4]);
    let disc_name = {
        let raw = unquote(&fields[5]);
        if raw.is_empty() {
            None
        } else {
            Some(raw)
        }
    };

    Some(ParsedDrive {
        index,
        visible,
        enabled,
        flags,
        drive_name,
        disc_name,
    })
}

/// Parse a `PRGV:` line from `makemkvcon -r` output.
///
/// Format: `PRGV:current,total,max`
pub fn parse_progress_line(line: &str) -> Option<ParsedProgress> {
    let rest = split_prefix(line, "PRGV:")?;
    let fields: Vec<&str> = rest.split(',').collect();
    if fields.len() < 3 {
        return None;
    }

    Some(ParsedProgress {
        current: fields[0].parse().ok()?,
        total: fields[1].parse().ok()?,
        max: fields[2].parse().ok()?,
    })
}

/// Parse a `TCOUT:` line from `makemkvcon -r` output.
///
/// Format: `TCOUT:count`
pub fn parse_title_count(line: &str) -> Option<usize> {
    let rest = split_prefix(line, "TCOUT:")?;
    rest.parse().ok()
}

/// Parse a `MSG:` line from `makemkvcon -r` output.
///
/// Format: `MSG:code,flags,count,"message","format",param0,...`
pub fn parse_message_line(line: &str) -> Option<ParsedMessage> {
    let rest = split_prefix(line, "MSG:")?;
    let fields = parse_csv_fields(rest);
    if fields.len() < 4 {
        return None;
    }

    let code = fields[0].parse().ok()?;
    let flags = fields[1].parse().ok()?;
    let message = unquote(&fields[3]);

    Some(ParsedMessage {
        code,
        flags,
        message,
    })
}

/// Check if a message line indicates a critical error (code 3010xx).
pub fn is_error_message(line: &str) -> bool {
    if let Some(msg) = parse_message_line(line) {
        // MakeMKV error codes start with 3010
        msg.code >= 301000 && msg.code < 301100
    } else {
        false
    }
}

/// Check if a line indicates the rip/backup is complete.
/// MakeMKV outputs MSG:201003,0,0 when done.
pub fn is_completion_line(line: &str) -> bool {
    line.starts_with("MSG:201003,")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- DRV parsing ---

    #[test]
    fn test_parse_drive_with_disc() {
        let line = r#"DRV:0,1,1,24,"HL-DT-ST BD-RE BH16NS40","INCEPTION""#;
        let drive = parse_drive_line(line).unwrap();
        assert_eq!(drive.index, 0);
        assert!(drive.visible);
        assert!(drive.enabled);
        assert_eq!(drive.flags, 24);
        assert_eq!(drive.drive_name, "HL-DT-ST BD-RE BH16NS40");
        assert_eq!(drive.disc_name.as_deref(), Some("INCEPTION"));
    }

    #[test]
    fn test_parse_drive_empty() {
        let line = r#"DRV:1,1,1,0,"TSSTcorp CDDVDW SH-224DB","""#;
        let drive = parse_drive_line(line).unwrap();
        assert_eq!(drive.index, 1);
        assert!(drive.visible);
        assert!(drive.enabled);
        assert_eq!(drive.drive_name, "TSSTcorp CDDVDW SH-224DB");
        assert!(drive.disc_name.is_none());
    }

    #[test]
    fn test_parse_drive_not_visible() {
        let line = r#"DRV:2,0,0,0,"Virtual Drive","#;
        let drive = parse_drive_line(line).unwrap();
        assert_eq!(drive.index, 2);
        assert!(!drive.visible);
        assert!(!drive.enabled);
    }

    #[test]
    fn test_parse_drive_invalid_prefix() {
        assert!(parse_drive_line("MSG:123,0,0").is_none());
        assert!(parse_drive_line("hello").is_none());
        assert!(parse_drive_line("").is_none());
    }

    #[test]
    fn test_parse_drive_too_few_fields() {
        assert!(parse_drive_line("DRV:0,1,1").is_none());
    }

    #[test]
    fn test_parse_drive_with_quotes_in_name() {
        let line = r#"DRV:0,1,1,8,"LG Electronics BP-LG","The \"Best\" Movie""#;
        let drive = parse_drive_line(line).unwrap();
        assert_eq!(drive.drive_name, "LG Electronics BP-LG");
        assert_eq!(drive.disc_name.as_deref(), Some("The \"Best\" Movie"));
    }

    // --- PRGV parsing ---

    #[test]
    fn test_parse_progress() {
        let prog = parse_progress_line("PRGV:5000,10000,100000").unwrap();
        assert_eq!(prog.current, 5000);
        assert_eq!(prog.total, 10000);
        assert_eq!(prog.max, 100000);
    }

    #[test]
    fn test_parse_progress_zero() {
        let prog = parse_progress_line("PRGV:0,0,100000").unwrap();
        assert_eq!(prog.current, 0);
        assert_eq!(prog.total, 0);
        assert_eq!(prog.max, 100000);
    }

    #[test]
    fn test_parse_progress_complete() {
        let prog = parse_progress_line("PRGV:100000,100000,100000").unwrap();
        assert_eq!(prog.current, 100000);
        assert_eq!(prog.total, 100000);
    }

    #[test]
    fn test_parse_progress_invalid() {
        assert!(parse_progress_line("PRGV:abc,def,ghi").is_none());
        assert!(parse_progress_line("PRGV:1,2").is_none());
        assert!(parse_progress_line("hello").is_none());
    }

    // --- TCOUT parsing ---

    #[test]
    fn test_parse_title_count() {
        assert_eq!(parse_title_count("TCOUT:5"), Some(5));
        assert_eq!(parse_title_count("TCOUT:0"), Some(0));
        assert_eq!(parse_title_count("TCOUT:42"), Some(42));
    }

    #[test]
    fn test_parse_title_count_invalid() {
        assert!(parse_title_count("TCOUT:abc").is_none());
        assert!(parse_title_count("hello").is_none());
    }

    // --- MSG parsing ---

    #[test]
    fn test_parse_message() {
        let msg = parse_message_line(
            r#"MSG:201003,0,0,"Copy complete.","MakeMKV copy complete.""#,
        )
        .unwrap();
        assert_eq!(msg.code, 201003);
        assert_eq!(msg.flags, 0);
        assert_eq!(msg.message, "Copy complete.");
    }

    #[test]
    fn test_parse_message_with_params() {
        let msg = parse_message_line(
            r#"MSG:301001,0,1,"No disc found.","Drive %1 has no disc.""#,
        )
        .unwrap();
        assert_eq!(msg.code, 301001);
        assert_eq!(msg.message, "No disc found.");
    }

    #[test]
    fn test_parse_message_invalid() {
        assert!(parse_message_line("hello").is_none());
        assert!(parse_message_line("MSG:abc").is_none());
    }

    // --- Error/completion detection ---

    #[test]
    fn test_is_error_message() {
        assert!(is_error_message(
            r#"MSG:301001,0,0,"Error","Error occurred""#
        ));
        assert!(!is_error_message(
            r#"MSG:201003,0,0,"Done","Complete""#
        ));
        assert!(!is_error_message("hello"));
    }

    #[test]
    fn test_is_completion_line() {
        assert!(is_completion_line(
            "MSG:201003,0,0,\"Copy complete.\""
        ));
        assert!(!is_completion_line(
            "MSG:301001,0,0,\"Error\""
        ));
        assert!(!is_completion_line("PRGV:100,100,100"));
    }

    // --- Unquote ---

    #[test]
    fn test_unquote() {
        assert_eq!(unquote(r#""hello""#), "hello");
        assert_eq!(unquote("hello"), "hello");
        assert_eq!(unquote(r#""he said \"hi\"""#), r#"he said "hi""#);
        assert_eq!(unquote(r#""back\\slash""#), r#"back\slash"#);
        assert_eq!(unquote(r#"" ""#), " ");
    }

    // --- CSV parsing ---

    #[test]
    fn test_parse_csv_simple() {
        let fields = parse_csv_fields("a,b,c");
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_csv_quoted() {
        let fields = parse_csv_fields(r#"1,2,"hello, world",4"#);
        assert_eq!(fields, vec!["1", "2", r#""hello, world""#, "4"]);
    }

    // --- Integration: multi-line output ---

    #[test]
    fn test_full_drive_listing_output() {
        let output = r#"MSG:0,0,0,"Using libmakemkv 1.18.4 (26) for x86_64-linux-gnu","(version)"
DRV:0,1,1,24,"HL-DT-ST BD-RE BH16NS40","INCEPTION_2010"
DRV:1,1,1,16,"TSSTcorp CDDVDW SH-224DB","THE_DARK_KNIGHT"
DRV:2,0,0,0,"Virtual Drive","#;

        let drives: Vec<_> = output
            .lines()
            .filter_map(parse_drive_line)
            .collect();

        assert_eq!(drives.len(), 3);
        assert_eq!(drives[0].disc_name.as_deref(), Some("INCEPTION_2010"));
        assert_eq!(drives[1].disc_name.as_deref(), Some("THE_DARK_KNIGHT"));
        assert!(drives[2].disc_name.is_none());
    }

    #[test]
    fn test_full_rip_progress_output() {
        let lines = vec![
            "MSG:0,0,0,\"Starting copy...\"",
            "PRGV:0,0,100000",
            "PRGV:25000,25000,100000",
            "PRGV:50000,50000,100000",
            "PRGV:75000,75000,100000",
            "PRGV:100000,100000,100000",
            "MSG:201003,0,0,\"Copy complete.\"",
        ];

        let mut progress_values = Vec::new();
        let mut completed = false;

        for line in &lines {
            if let Some(prog) = parse_progress_line(line) {
                progress_values.push(prog);
            }
            if is_completion_line(line) {
                completed = true;
            }
        }

        assert_eq!(progress_values.len(), 5);
        assert_eq!(progress_values.last().unwrap().current, 100000);
        assert!(completed);
    }
}
