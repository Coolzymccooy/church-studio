use super::{default_strip_name, MAX_SCENE_NAME_CHARS, MAX_STRIP_NAME_CHARS};

/// Trim, bound and check a strip name. Empty means "use the default name".
pub fn clean_strip_name(index: usize, name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(default_strip_name(index));
    }
    if name.chars().count() > MAX_STRIP_NAME_CHARS {
        return Err(format!(
            "strip name is longer than {MAX_STRIP_NAME_CHARS} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("strip name contains control characters".to_string());
    }
    Ok(name.to_string())
}

/// Validate a scene name and turn it into a file-name slug.
///
/// Names are 1–40 characters (after trimming) of ASCII letters, digits,
/// space, `-` and `_`. The slug is lowercase with spaces replaced by `-`.
/// Anything else (dots, slashes, unicode) is rejected, so a slug can never
/// escape the scenes directory. Windows device names are rejected too.
pub fn scene_slug(name: &str) -> Result<String, String> {
    let name = name.trim();
    let len = name.chars().count();
    if len == 0 {
        return Err("scene name is empty".to_string());
    }
    if len > MAX_SCENE_NAME_CHARS {
        return Err(format!(
            "scene name is longer than {MAX_SCENE_NAME_CHARS} characters"
        ));
    }
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_');
    if !valid {
        return Err("scene name may only contain letters, digits, spaces, '-' and '_'".to_string());
    }
    let slug: String = name
        .chars()
        .map(|c| {
            if c == ' ' {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    if is_reserved_file_name(&slug) {
        return Err(format!("'{name}' is a reserved name"));
    }
    Ok(slug)
}

fn is_reserved_file_name(slug: &str) -> bool {
    const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];
    if RESERVED.contains(&slug) {
        return true;
    }
    let bytes = slug.as_bytes();
    bytes.len() == 4
        && (slug.starts_with("com") || slug.starts_with("lpt"))
        && (b'1'..=b'9').contains(&bytes[3])
}
