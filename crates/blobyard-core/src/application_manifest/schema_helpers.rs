use super::schema::Validator;
use toml::Value;
use toml::map::Map;

impl Validator {
    pub(super) fn required_pattern(
        &mut self,
        table: &Map<String, Value>,
        path: &str,
        key: &str,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        if let Some(value) = self.required(table, path, key) {
            self.pattern(value, &format!("{path}.{key}"), predicate, expected);
        }
    }

    pub(super) fn optional_pattern(
        &mut self,
        table: &Map<String, Value>,
        path: &str,
        key: &str,
        predicate: fn(&str) -> bool,
        expected: &str,
    ) {
        if let Some(value) = table.get(key) {
            self.pattern(value, &format!("{path}.{key}"), predicate, expected);
        }
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
}
