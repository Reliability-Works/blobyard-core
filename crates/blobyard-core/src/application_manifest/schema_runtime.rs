use super::patterns;
use super::schema::Validator;
use toml::Value;
use toml::map::Map;

pub(super) fn validate(validator: &mut Validator, root: &Map<String, Value>) {
    buckets(validator, root);
    functions(validator, root);
}

fn buckets(validator: &mut Validator, root: &Map<String, Value>) {
    let Some(value) = root.get("buckets") else {
        return;
    };
    let Some(items) = validator.array(value, "buckets") else {
        return;
    };
    validator.maximum_items(items, "buckets", 16);
    for (index, value) in items.iter().enumerate() {
        let path = format!("buckets[{index}]");
        let Some(table) = validator.table(value, &path, "an object") else {
            continue;
        };
        validator.unknown(table, &path, &["name", "visibility", "max_object_size"]);
        validator.required_pattern(table, &path, "name", patterns::dns_label, "a DNS label");
        if let Some(value) = table.get("visibility") {
            validator.enumeration(
                value,
                &format!("{path}.visibility"),
                &["private", "public-read"],
            );
        }
        validator.optional_pattern(
            table,
            &path,
            "max_object_size",
            patterns::byte_size,
            "a byte size such as 50MiB with 1-6 nonzero-leading digits",
        );
    }
}

fn functions(validator: &mut Validator, root: &Map<String, Value>) {
    let Some(value) = root.get("functions") else {
        return;
    };
    let Some(items) = validator.array(value, "functions") else {
        return;
    };
    validator.maximum_items(items, "functions", 64);
    for (index, value) in items.iter().enumerate() {
        function(validator, value, &format!("functions[{index}]"));
    }
}

fn function(validator: &mut Validator, value: &Value, path: &str) {
    let Some(table) = validator.table(value, path, "an object") else {
        return;
    };
    validator.unknown(
        table,
        path,
        &[
            "name",
            "entry",
            "type",
            "permissions",
            "database",
            "buckets",
            "secrets",
            "network",
            "email",
            "event",
            "queue",
        ],
    );
    validator.required_pattern(table, path, "name", patterns::dns_label, "a DNS label");
    validator.required_pattern(
        table,
        path,
        "entry",
        patterns::module_path,
        "a relative .ts, .js, .mts, or .mjs module path of at most 256 characters",
    );
    if let Some(value) = validator.required(table, path, "type") {
        validator.enumeration(
            value,
            &format!("{path}.type"),
            &["rpc", "http", "webhook", "event", "scheduled", "queue"],
        );
    }
    function_options(validator, table, path);
}

fn function_options(validator: &mut Validator, table: &Map<String, Value>, path: &str) {
    validator.optional_string_array(
        table,
        path,
        "permissions",
        16,
        patterns::permission,
        "a dotted permission of at most 128 characters",
    );
    if let Some(value) = table.get("database") {
        validator.enumeration(
            value,
            &format!("{path}.database"),
            &["none", "read", "read-write"],
        );
    }
    validator.optional_string_array(
        table,
        path,
        "buckets",
        16,
        patterns::bucket_grant,
        "a bucket grant",
    );
    validator.optional_string_array(
        table,
        path,
        "secrets",
        16,
        patterns::secret_name,
        "a secret name",
    );
    validator.optional_string_array(
        table,
        path,
        "network",
        16,
        patterns::egress_target,
        "a public DNS target on port 80 or 443",
    );
    validator.optional_boolean(table, path, "email");
    validator.optional_pattern(table, path, "event", patterns::permission, "an event name");
    validator.optional_pattern(table, path, "queue", patterns::dns_label, "a DNS label");
}
