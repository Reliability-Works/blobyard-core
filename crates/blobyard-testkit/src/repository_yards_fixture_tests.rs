use super::YardConformanceFixture;

#[test]
fn fixture_validates_each_distinct_yard_name() {
    assert!(YardConformanceFixture::new("primary", "inactive", "history").is_ok());
    assert!(YardConformanceFixture::new("", "inactive", "history").is_err());
    assert!(YardConformanceFixture::new("primary", "", "history").is_err());
    assert!(YardConformanceFixture::new("primary", "inactive", "").is_err());
}
