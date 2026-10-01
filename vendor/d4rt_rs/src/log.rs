use std::sync::atomic::{AtomicU8, Ordering};

static LOG_LEVEL: AtomicU8 = AtomicU8::new(1);

pub const ERROR: u8 = 0;
pub const WARN: u8 = 1;
pub const INFO: u8 = 2;
pub const DEBUG: u8 = 3;

pub fn set_level(level: u8) {
    LOG_LEVEL.store(level, Ordering::Relaxed);
}

pub fn enabled(level: u8) -> bool {
    level <= LOG_LEVEL.load(Ordering::Relaxed)
}

pub fn error(msg: &str) {
    if enabled(ERROR) {
        eprintln!("[d4rt_rs] ERROR: {msg}");
    }
}

pub fn warn(msg: &str) {
    if enabled(WARN) {
        eprintln!("[d4rt_rs] WARN: {msg}");
    }
}

pub fn info(msg: &str) {
    if enabled(INFO) {
        eprintln!("[d4rt_rs] INFO: {msg}");
    }
}

pub fn debug(msg: &str) {
    if enabled(DEBUG) {
        eprintln!("[d4rt_rs] DEBUG: {msg}");
    }
}
