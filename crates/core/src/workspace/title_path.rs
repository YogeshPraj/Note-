//! A file's path as the title bar shows it: whole when short, shortened in the middle when long.

/// The longest path the title bar shows before it is shortened.
pub(super) const TITLE_PATH: usize = 70;

/// `path` as it is when short; else its first part and the last parts that fit, joined with `...`. The file name always stays.
pub(super) fn short_path(path: &str, max: usize) -> String {
    if path.chars().count() <= max {
        return path.to_string();
    }
    let sep = if path.contains('\\') { '\\' } else { '/' };
    let parts: Vec<&str> = path.split(sep).collect();
    if parts.len() < 3 {
        return path.to_string();
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    let mut tail = vec![last];
    let mut len = first.chars().count() + 5 + last.chars().count(); // first, separator, "...", separator, name
    for part in parts[1..parts.len() - 1].iter().rev() {
        let add = part.chars().count() + 1;
        if len + add > max {
            break;
        }
        len += add;
        tail.insert(0, part);
    }
    format!("{first}{sep}...{sep}{}", tail.join(&sep.to_string()))
}

#[cfg(test)]
mod tests {
    use super::short_path;

    #[test]
    fn a_long_path_keeps_its_drive_and_its_last_parts() {
        let p = r"C:\src\crm.cmsp.consumer-uat-ka-profile\foundry\src\CMSP.Consumer.Foundry.Api\config-store\routing\public-profiles\default.json";
        let s = short_path(p, 70);
        assert!(s.starts_with(r"C:\...\") && s.ends_with(r"public-profiles\default.json"), "{s}");
        assert!(s.chars().count() <= 70, "{s}");
    }

    #[test]
    fn short_paths_are_left_alone() {
        assert_eq!(short_path(r"C:\a\b.txt", 70), r"C:\a\b.txt");
        assert_eq!(short_path("notes.txt", 3), "notes.txt");
    }
}
