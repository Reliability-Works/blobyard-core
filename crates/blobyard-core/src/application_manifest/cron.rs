#[derive(Clone, Copy)]
struct Field<'a> {
    source: &'a str,
    minimum: u8,
    maximum: u8,
    names: &'static [&'static str],
}

pub(super) fn valid(value: &str) -> bool {
    let fields = value.split(' ').collect::<Vec<_>>();
    if fields.len() != 5 {
        return false;
    }
    let specifications = [
        Field::new(fields[0], 0, 59, &[]),
        Field::new(fields[1], 0, 23, &[]),
        Field::new(fields[2], 1, 31, &[]),
        Field::new(
            fields[3],
            1,
            12,
            &[
                "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
            ],
        ),
        Field::new(
            fields[4],
            0,
            7,
            &["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"],
        ),
    ];
    specifications.iter().all(Field::valid)
}

impl<'a> Field<'a> {
    const fn new(
        source: &'a str,
        minimum: u8,
        maximum: u8,
        names: &'static [&'static str],
    ) -> Self {
        Self {
            source,
            minimum,
            maximum,
            names,
        }
    }

    fn valid(&self) -> bool {
        !self.source.is_empty() && self.source.split(',').all(|item| self.valid_item(item))
    }

    fn valid_item(&self, item: &str) -> bool {
        let mut parts = item.split('/');
        let base = parts.next().unwrap_or_default();
        let step = parts.next();
        if parts.next().is_some() || step.is_some_and(|value| !valid_step(value)) {
            return false;
        }
        if base == "*" {
            return true;
        }
        let mut range = base.split('-');
        let Some(start) = range.next().and_then(|value| self.value(value)) else {
            return false;
        };
        let end = range.next();
        if range.next().is_some() {
            return false;
        }
        end.is_none_or(|value| self.value(value).is_some_and(|end| start <= end))
    }

    fn value(&self, value: &str) -> Option<u8> {
        value
            .parse::<u8>()
            .ok()
            .filter(|number| (self.minimum..=self.maximum).contains(number))
            .or_else(|| {
                self.names
                    .iter()
                    .position(|name| value.eq_ignore_ascii_case(name))
                    .and_then(|index| u8::try_from(index).ok())
                    .and_then(|index| index.checked_add(self.minimum))
            })
    }
}

fn valid_step(value: &str) -> bool {
    value.parse::<u16>().is_ok_and(|step| step > 0)
}
