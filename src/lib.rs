p// src/lib.rs

#[no_mangle]
pub extern "C" fn init_shared_memory() -> i32 {
    // Shared memory allocation logic
    0
}
pub mod data_struct;
pub mod history;
pub mod pipeln_seq_io;
