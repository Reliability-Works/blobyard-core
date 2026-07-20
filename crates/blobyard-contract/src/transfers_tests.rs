use super::ReservationStrategy;

#[test]
fn reservation_strategy_rejects_unknown_persisted_values() {
    assert_eq!(ReservationStrategy::parse("unknown"), None);
}
