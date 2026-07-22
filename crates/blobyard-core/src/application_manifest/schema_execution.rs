use super::patterns;
use super::schema::Validator;
use toml::Value;
use toml::map::Map;

pub(super) fn validate(validator: &mut Validator, root: &Map<String, Value>) {
    jobs(validator, root);
    routes(validator, root);
    limits(validator, root);
    health(validator, root);
}

fn jobs(validator: &mut Validator, root: &Map<String, Value>) {
    let Some(value) = root.get("jobs") else {
        return;
    };
    let Some(items) = validator.array(value, "jobs") else {
        return;
    };
    validator.maximum_items(items, "jobs", 32);
    for (index, value) in items.iter().enumerate() {
        job(validator, value, &format!("jobs[{index}]"));
    }
}

fn job(validator: &mut Validator, value: &Value, path: &str) {
    let Some(table) = validator.table(value, path, "an object") else {
        return;
    };
    validator.unknown(
        table,
        path,
        &["name", "function", "schedule", "timezone", "retry"],
    );
    validator.required_pattern(table, path, "name", patterns::dns_label, "a DNS label");
    validator.required_pattern(table, path, "function", patterns::dns_label, "a DNS label");
    validator.required_pattern(
        table,
        path,
        "schedule",
        patterns::cron_shape,
        "exactly five space-separated cron fields of at most 64 characters",
    );
    validator.required_pattern(
        table,
        path,
        "timezone",
        patterns::timezone_shape,
        "UTC or an IANA-shaped timezone of at most 64 characters",
    );
    if let Some(value) = table.get("retry") {
        retry(validator, value, &format!("{path}.retry"));
    }
}

fn retry(validator: &mut Validator, value: &Value, path: &str) {
    let Some(table) = validator.table(value, path, "an object") else {
        return;
    };
    validator.unknown(table, path, &["max_attempts", "backoff"]);
    if let Some(value) = table.get("max_attempts") {
        validator.integer(value, &format!("{path}.max_attempts"), 1, 10);
    }
    if let Some(value) = table.get("backoff") {
        validator.enumeration(value, &format!("{path}.backoff"), &["fixed", "exponential"]);
    }
}

fn routes(validator: &mut Validator, root: &Map<String, Value>) {
    let Some(value) = root.get("routes") else {
        return;
    };
    let Some(items) = validator.array(value, "routes") else {
        return;
    };
    validator.maximum_items(items, "routes", 64);
    for (index, value) in items.iter().enumerate() {
        route(validator, value, &format!("routes[{index}]"));
    }
}

fn route(validator: &mut Validator, value: &Value, path: &str) {
    let Some(table) = validator.table(value, path, "an object") else {
        return;
    };
    validator.unknown(table, path, &["path", "method", "function", "auth"]);
    validator.required_pattern(
        table,
        path,
        "path",
        patterns::route_path,
        "an absolute normalized route path of at most 256 characters",
    );
    if let Some(value) = validator.required(table, path, "method") {
        validator.enumeration(
            value,
            &format!("{path}.method"),
            &["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"],
        );
    }
    validator.required_pattern(table, path, "function", patterns::dns_label, "a DNS label");
    if let Some(value) = table.get("auth") {
        validator.enumeration(value, &format!("{path}.auth"), &["required", "public"]);
    }
}

fn limits(validator: &mut Validator, root: &Map<String, Value>) {
    let Some(value) = root.get("limits") else {
        return;
    };
    let Some(table) = validator.table(value, "limits", "an object") else {
        return;
    };
    validator.unknown(
        table,
        "limits",
        &["function_class", "function_timeout", "concurrency"],
    );
    if let Some(value) = table.get("function_class") {
        validator.enumeration(value, "limits.function_class", &["standard"]);
    }
    validator.optional_pattern(
        table,
        "limits",
        "function_timeout",
        patterns::duration,
        "a duration with 1-4 nonzero-leading digits and ms, s, or m",
    );
    if let Some(value) = table.get("concurrency") {
        validator.integer(value, "limits.concurrency", 1, 100);
    }
}

fn health(validator: &mut Validator, root: &Map<String, Value>) {
    let Some(value) = root.get("health") else {
        return;
    };
    let Some(table) = validator.table(value, "health", "an object") else {
        return;
    };
    validator.unknown(table, "health", &["function", "timeout"]);
    validator.required_pattern(
        table,
        "health",
        "function",
        patterns::dns_label,
        "a DNS label",
    );
    validator.optional_pattern(
        table,
        "health",
        "timeout",
        patterns::duration,
        "a duration with 1-4 nonzero-leading digits and ms, s, or m",
    );
}
