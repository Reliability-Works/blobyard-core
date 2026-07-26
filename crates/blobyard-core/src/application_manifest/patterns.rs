pub(super) fn dns_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && edge_alphanumeric(value)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn role_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn permission(value: &str) -> bool {
    value.len() <= 128 && dotted_identifier(value)
}

pub(super) fn secret_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_uppercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

pub(super) fn bucket_grant(value: &str) -> bool {
    value.split_once(':').is_some_and(|(name, access)| {
        dns_label(name) && matches!(access, "read" | "write" | "read-write")
    })
}

pub(super) fn egress_target(value: &str) -> bool {
    if value.len() > 256 {
        return false;
    }
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    let labels = host.split('.').collect::<Vec<_>>();
    matches!(port, "80" | "443")
        && labels.len() >= 2
        && labels.last().is_some_and(|tld| {
            (2..=63).contains(&tld.len()) && tld.bytes().all(|byte| byte.is_ascii_lowercase())
        })
        && labels.iter().all(|label| host_label(label))
}

pub(super) fn relative_path(value: &str) -> bool {
    value.len() <= 256
        && !value.is_empty()
        && !value.starts_with('/')
        && value.split('/').all(path_segment)
}

pub(super) fn module_path(value: &str) -> bool {
    if value.len() > 256 {
        return false;
    }
    [".ts", ".js", ".mts", ".mjs"]
        .iter()
        .any(|suffix| value.strip_suffix(suffix).is_some_and(relative_path))
}

pub(super) fn route_path(value: &str) -> bool {
    if value.len() > 256 || !value.starts_with('/') {
        return false;
    }
    let tail = &value[1..];
    tail.is_empty()
        || tail.split('/').all(|segment| {
            !segment.is_empty()
                && segment.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-')
                })
        })
}

pub(super) fn byte_size(value: &str) -> bool {
    unit_number(value, &["KiB", "MiB", "GiB"], 6)
}

pub(super) fn duration(value: &str) -> bool {
    unit_number(value, &["ms", "s", "m"], 4)
}

pub(super) fn cron_shape(value: &str) -> bool {
    if value.len() > 64 {
        return false;
    }
    let fields = value.split(' ').collect::<Vec<_>>();
    fields.len() == 5
        && fields
            .iter()
            .all(|field| !field.is_empty() && !field.chars().any(char::is_whitespace))
}

pub(super) fn timezone_shape(value: &str) -> bool {
    if value == "UTC" {
        return true;
    }
    if value.len() > 64 {
        return false;
    }
    let parts = value.split('/').collect::<Vec<_>>();
    (2..=3).contains(&parts.len())
        && parts[0]
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && parts[1..].iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-'))
        })
}

fn edge_alphanumeric(value: &str) -> bool {
    value.as_bytes()[0].is_ascii_alphanumeric()
        && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
}

fn dotted_identifier(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    (2..=4).contains(&parts.len())
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.as_bytes()[0].is_ascii_lowercase()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
}

fn host_label(value: &str) -> bool {
    !value.is_empty()
        && edge_alphanumeric(value)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn path_segment(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn unit_number(value: &str, units: &[&str], maximum_digits: usize) -> bool {
    units.iter().any(|unit| {
        value.strip_suffix(unit).is_some_and(|number| {
            !number.is_empty()
                && number.len() <= maximum_digits
                && number.as_bytes()[0].is_ascii_digit()
                && number.as_bytes()[0] != b'0'
                && number.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}
