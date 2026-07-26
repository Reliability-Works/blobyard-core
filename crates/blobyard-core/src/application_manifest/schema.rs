use super::ManifestError;
use super::patterns;
use toml::Value;
use toml::map::Map;

pub(super) fn validate(value: &Value) -> Vec<ManifestError> {
    let mut validator = Validator::default();
    validator.root(value);
    validator.errors
}

#[derive(Default)]
pub(super) struct Validator {
    pub(super) errors: Vec<ManifestError>,
}

impl Validator {
    fn root(&mut self, value: &Value) {
        let Some(table) = self.table(value, "$", "an object") else {
            return;
        };
        self.unknown(
            table,
            "",
            &[
                "schema_version",
                "application",
                "frontend",
                "auth",
                "database",
                "buckets",
                "functions",
                "jobs",
                "routes",
                "limits",
                "health",
            ],
        );
        self.schema_version(table);
        self.application(table);
        self.frontend(table);
        self.auth(table);
        self.database(table);
        super::schema_runtime::validate(self, table);
        super::schema_execution::validate(self, table);
    }

    fn schema_version(&mut self, table: &Map<String, Value>) {
        let Some(value) = self.required(table, "", "schema_version") else {
            return;
        };
        match value.as_integer() {
            Some(1) => {}
            Some(_) => self.error("schema_version", "must equal 1"),
            None => self.error("schema_version", "must be an integer"),
        }
    }

    fn application(&mut self, root: &Map<String, Value>) {
        let Some(value) = self.required(root, "", "application") else {
            return;
        };
        let Some(table) = self.table(value, "application", "an object") else {
            return;
        };
        self.unknown(table, "application", &["name", "runtime"]);
        if let Some(value) = self.required(table, "application", "name") {
            self.pattern(
                value,
                "application.name",
                patterns::dns_label,
                "a DNS label",
            );
        }
        if let Some(value) = self.required(table, "application", "runtime") {
            self.enumeration(value, "application.runtime", &["blobyard-js-1"]);
        }
    }

    fn frontend(&mut self, root: &Map<String, Value>) {
        let Some(value) = root.get("frontend") else {
            return;
        };
        let Some(table) = self.table(value, "frontend", "an object") else {
            return;
        };
        self.unknown(
            table,
            "frontend",
            &["directory", "spa_fallback", "clean_urls"],
        );
        if let Some(value) = self.required(table, "frontend", "directory") {
            self.pattern(
                value,
                "frontend.directory",
                patterns::relative_path,
                "a portable relative path of at most 256 characters",
            );
        }
        self.optional_boolean(table, "frontend", "spa_fallback");
        self.optional_boolean(table, "frontend", "clean_urls");
    }

    fn auth(&mut self, root: &Map<String, Value>) {
        let Some(value) = root.get("auth") else {
            return;
        };
        let Some(table) = self.table(value, "auth", "an object") else {
            return;
        };
        self.unknown(table, "auth", &["default_role", "roles"]);
        if let Some(value) = table.get("default_role") {
            self.pattern(
                value,
                "auth.default_role",
                patterns::role_name,
                "a role name",
            );
        }
        if let Some(value) = table.get("roles") {
            self.roles(value);
        }
    }

    fn roles(&mut self, value: &Value) {
        let Some(roles) = self.table(value, "auth.roles", "an object") else {
            return;
        };
        self.property_count(roles, "auth.roles", 1, 32);
        for (name, value) in roles {
            let path = format!("auth.roles.{name}");
            if !patterns::role_name(name) {
                self.error(&path, "role name must match ^[a-z][a-z0-9-]{0,31}$");
            }
            self.role(value, &path);
        }
    }

    fn role(&mut self, value: &Value, path: &str) {
        let Some(table) = self.table(value, path, "an object") else {
            return;
        };
        self.unknown(table, path, &["inherits", "permissions"]);
        if let Some(value) = table.get("inherits") {
            self.string_array(
                value,
                &format!("{path}.inherits"),
                8,
                patterns::role_name,
                "a role name",
            );
        }
        if let Some(value) = table.get("permissions") {
            self.string_array(
                value,
                &format!("{path}.permissions"),
                64,
                patterns::permission,
                "a dotted permission of at most 128 characters",
            );
        }
    }

    fn database(&mut self, root: &Map<String, Value>) {
        let Some(value) = root.get("database") else {
            return;
        };
        let Some(table) = self.table(value, "database", "an object") else {
            return;
        };
        self.unknown(table, "database", &["migrations"]);
        if let Some(value) = self.required(table, "database", "migrations") {
            self.pattern(
                value,
                "database.migrations",
                patterns::relative_path,
                "a portable relative path of at most 256 characters",
            );
        }
    }

    pub(super) fn table<'a>(
        &mut self,
        value: &'a Value,
        path: &str,
        expected: &str,
    ) -> Option<&'a Map<String, Value>> {
        value.as_table().or_else(|| {
            self.error(path, format!("must be {expected}"));
            None
        })
    }

    pub(super) fn array<'a>(&mut self, value: &'a Value, path: &str) -> Option<&'a [Value]> {
        value.as_array().map(Vec::as_slice).or_else(|| {
            self.error(path, "must be an array");
            None
        })
    }

    pub(super) fn required<'a>(
        &mut self,
        table: &'a Map<String, Value>,
        parent: &str,
        key: &str,
    ) -> Option<&'a Value> {
        table.get(key).or_else(|| {
            self.error(join(parent, key), "is required");
            None
        })
    }

    pub(super) fn unknown(&mut self, table: &Map<String, Value>, path: &str, allowed: &[&str]) {
        for key in table.keys().filter(|key| !allowed.contains(&key.as_str())) {
            self.error(join(path, key), "unknown field for schema_version 1");
        }
    }

    pub(super) fn pattern(
        &mut self,
        value: &Value,
        path: &str,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        match value.as_str() {
            Some(text) if predicate(text) => {}
            Some(_) => self.error(path, format!("must be {expected}")),
            None => self.error(path, "must be a string"),
        }
    }

    pub(super) fn enumeration(&mut self, value: &Value, path: &str, allowed: &[&str]) {
        match value.as_str() {
            Some(text) if allowed.contains(&text) => {}
            Some(_) => self.error(path, format!("must be one of: {}", allowed.join(", "))),
            None => self.error(path, "must be a string"),
        }
    }

    pub(super) fn optional_boolean(&mut self, table: &Map<String, Value>, path: &str, key: &str) {
        if table.get(key).is_some_and(|value| !value.is_bool()) {
            self.error(join(path, key), "must be a boolean");
        }
    }

    pub(super) fn integer(&mut self, value: &Value, path: &str, minimum: i64, maximum: i64) {
        match value.as_integer() {
            Some(number) if (minimum..=maximum).contains(&number) => {}
            Some(_) => self.error(path, format!("must be between {minimum} and {maximum}")),
            None => self.error(path, "must be an integer"),
        }
    }

    pub(super) fn string_array(
        &mut self,
        value: &Value,
        path: &str,
        maximum: usize,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        let Some(items) = self.array(value, path) else {
            return;
        };
        self.maximum_items(items, path, maximum);
        self.unique_items(items, path);
        for (index, item) in items.iter().enumerate() {
            self.pattern(item, &format!("{path}[{index}]"), predicate, expected);
        }
    }

    pub(super) fn maximum_items(&mut self, items: &[Value], path: &str, maximum: usize) {
        if items.len() > maximum {
            self.error(path, format!("must contain at most {maximum} items"));
        }
    }

    fn unique_items(&mut self, items: &[Value], path: &str) {
        for (index, item) in items.iter().enumerate() {
            if items[..index].contains(item) {
                self.error(format!("{path}[{index}]"), "must be unique");
            }
        }
    }

    fn property_count(&mut self, table: &Map<String, Value>, path: &str, min: usize, max: usize) {
        if !(min..=max).contains(&table.len()) {
            self.error(
                path,
                format!("must contain between {min} and {max} properties"),
            );
        }
    }

    pub(super) fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.errors.push(ManifestError::new(path, message));
    }
}

fn join(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_owned()
    } else {
        format!("{parent}.{key}")
    }
}
