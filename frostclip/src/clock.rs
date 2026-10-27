use std::sync::OnceLock;
use std::time::Instant;

static EPOCH: OnceLock<Instant> = OnceLock::new();

pub fn now_us() -> i64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as i64
}
