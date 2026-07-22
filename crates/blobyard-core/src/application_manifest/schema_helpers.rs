use super::schema::Validator;
use toml::Value;
use toml::map::Map;

impl Validator {
    pub(super) fn object<'a>(
        &mut self,
        value: &'a Value,
        path: &str,
        allowed: &[&str],
    ) -> Option<&'a Map<String, Value>> {
        let table = self.table(value, path, "an object")?;
        self.unknown(table, path, allowed);
        Some(table)
    }

    pub(super) fn required_pattern(
        &mut self,
        table: &Map<String, Value>,
        path: &str,
        key: &str,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        let value = self.required(table, path, key);
        self.field_pattern(value, path, key, predicate, expected);
    }

    pub(super) fn optional_pattern(
        &mut self,
        table: &Map<String, Value>,
        path: &str,
        key: &str,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        self.field_pattern(table.get(key), path, key, predicate, expected);
    }

    pub(super) fn optional_string_array(
        &mut self,
        table: &Map<String, Value>,
        path: &str,
        key: &str,
        maximum: usize,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        if let Some(value) = table.get(key) {
            self.string_array(
                value,
                &format!("{path}.{key}"),
                maximum,
                predicate,
                expected,
            );
        }
    }

    fn field_pattern(
        &mut self,
        value: Option<&Value>,
        path: &str,
        key: &str,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        if let Some(value) = value {
            self.pattern(value, &format!("{path}.{key}"), predicate, expected);
        }
    }
}
