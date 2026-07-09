use zenith_lib::alarms::halo::load_alarms;

#[test]
fn test_load_alarms() {
    let events = load_alarms();
    println!("Found {} alarms:", events.len());
    for (i, ev) in events.iter().enumerate() {
        println!("  {}: {:?}", i, ev);
    }
}
