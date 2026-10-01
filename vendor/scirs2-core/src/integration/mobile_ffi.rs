//! Mobile FFI (Foreign Function Interface) bindings
//!
//! This module provides C-ABI compatible bindings for mobile platforms:
//! - Swift FFI for iOS
//! - JNI for Android (Java/Kotlin)
//!
//! ## Safety
//!
//! All FFI functions are marked `unsafe` and require careful memory management.
//! Callers must ensure:
//! - Pointers are valid and properly aligned
//! - Buffer sizes are correct
//! - Memory is not freed while in use

use std::ffi::CStr;
use std::os::raw::{c_char, c_float, c_int, c_void};
use std::slice;

/// C-compatible error code
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SciFfiError {
    Success = 0,
    InvalidArgument = 1,
    NullPointer = 2,
    AllocationFailed = 3,
    ComputationFailed = 4,
    UnsupportedOperation = 5,
}

/// C-compatible vector descriptor
#[repr(C)]
pub struct SciFfiVector {
    pub data: *mut c_float,
    pub len: usize,
    pub capacity: usize,
}

/// C-compatible matrix descriptor
#[repr(C)]
pub struct SciFfiMatrix {
    pub data: *mut c_float,
    pub rows: usize,
    pub cols: usize,
    pub stride: usize,
}

/// Initialize SciRS2 library (must be called first)
#[no_mangle]
pub unsafe extern "C" fn scirs2_init() -> SciFfiError {
    // Initialize logging, thread pools, etc.
    SciFfiError::Success
}

/// Shutdown SciRS2 library (cleanup resources)
#[no_mangle]
pub unsafe extern "C" fn scirs2_shutdown() -> SciFfiError {
    SciFfiError::Success
}

// ============================================================================
// Vector Operations
// ============================================================================

/// Create a new vector with specified capacity
#[no_mangle]
pub unsafe extern "C" fn scirs2_vector_create(capacity: usize) -> *mut SciFfiVector {
    let data = libc::malloc(capacity * std::mem::size_of::<c_float>()) as *mut c_float;
    if data.is_null() {
        return std::ptr::null_mut();
    }

    let vec = Box::new(SciFfiVector {
        data,
        len: 0,
        capacity,
    });

    Box::into_raw(vec)
}

/// Destroy a vector and free its memory
#[no_mangle]
pub unsafe extern "C" fn scirs2_vector_destroy(vec: *mut SciFfiVector) {
    if !vec.is_null() {
        let vec = Box::from_raw(vec);
        if !vec.data.is_null() {
            libc::free(vec.data as *mut c_void);
        }
    }
}

/// Vector dot product: result = a · b
#[no_mangle]
pub unsafe extern "C" fn scirs2_vector_dot(
    a: *const c_float,
    b: *const c_float,
    len: usize,
    result: *mut c_float,
) -> SciFfiError {
    if a.is_null() || b.is_null() || result.is_null() {
        return SciFfiError::NullPointer;
    }

    let a_slice = slice::from_raw_parts(a, len);
    let b_slice = slice::from_raw_parts(b, len);

    let dot = crate::simd::neon::neon_dot_f32(a_slice, b_slice);
    *result = dot;

    SciFfiError::Success
}

/// Vector addition: out = a + b
#[no_mangle]
pub unsafe extern "C" fn scirs2_vector_add(
    a: *const c_float,
    b: *const c_float,
    out: *mut c_float,
    len: usize,
) -> SciFfiError {
    if a.is_null() || b.is_null() || out.is_null() {
        return SciFfiError::NullPointer;
    }

    let a_slice = slice::from_raw_parts(a, len);
    let b_slice = slice::from_raw_parts(b, len);
    let out_slice = slice::from_raw_parts_mut(out, len);

    crate::simd::neon::neon_add_f32(a_slice, b_slice, out_slice);

    SciFfiError::Success
}

/// Vector multiplication: out = a * b (element-wise)
#[no_mangle]
pub unsafe extern "C" fn scirs2_vector_mul(
    a: *const c_float,
    b: *const c_float,
    out: *mut c_float,
    len: usize,
) -> SciFfiError {
    if a.is_null() || b.is_null() || out.is_null() {
        return SciFfiError::NullPointer;
    }

    let a_slice = slice::from_raw_parts(a, len);
    let b_slice = slice::from_raw_parts(b, len);
    let out_slice = slice::from_raw_parts_mut(out, len);

    crate::simd::neon::neon_mul_f32(a_slice, b_slice, out_slice);

    SciFfiError::Success
}

// ============================================================================
// Matrix Operations
// ============================================================================

/// Create a new matrix with specified dimensions
#[no_mangle]
pub unsafe extern "C" fn scirs2_matrix_create(rows: usize, cols: usize) -> *mut SciFfiMatrix {
    let size = rows * cols;
    let data = libc::malloc(size * std::mem::size_of::<c_float>()) as *mut c_float;
    if data.is_null() {
        return std::ptr::null_mut();
    }

    let mat = Box::new(SciFfiMatrix {
        data,
        rows,
        cols,
        stride: cols,
    });

    Box::into_raw(mat)
}

/// Destroy a matrix and free its memory
#[no_mangle]
pub unsafe extern "C" fn scirs2_matrix_destroy(mat: *mut SciFfiMatrix) {
    if !mat.is_null() {
        let mat = Box::from_raw(mat);
        if !mat.data.is_null() {
            libc::free(mat.data as *mut c_void);
        }
    }
}

/// Matrix-vector multiplication: y = A * x
#[no_mangle]
pub unsafe extern "C" fn scirs2_matrix_vector_mul(
    a_data: *const c_float,
    a_rows: usize,
    a_cols: usize,
    x: *const c_float,
    y: *mut c_float,
) -> SciFfiError {
    if a_data.is_null() || x.is_null() || y.is_null() {
        return SciFfiError::NullPointer;
    }

    let a_slice = slice::from_raw_parts(a_data, a_rows * a_cols);
    let x_slice = slice::from_raw_parts(x, a_cols);
    let y_slice = slice::from_raw_parts_mut(y, a_rows);

    crate::simd::neon::neon_gemv_f32(a_rows, a_cols, 1.0, a_slice, x_slice, 0.0, y_slice);

    SciFfiError::Success
}

/// Matrix-matrix multiplication: C = A * B
#[no_mangle]
pub unsafe extern "C" fn scirs2_matrix_mul(
    a_data: *const c_float,
    a_rows: usize,
    a_cols: usize,
    b_data: *const c_float,
    b_rows: usize,
    b_cols: usize,
    c_data: *mut c_float,
) -> SciFfiError {
    if a_data.is_null() || b_data.is_null() || c_data.is_null() {
        return SciFfiError::NullPointer;
    }

    if a_cols != b_rows {
        return SciFfiError::InvalidArgument;
    }

    let a_slice = slice::from_raw_parts(a_data, a_rows * a_cols);
    let b_slice = slice::from_raw_parts(b_data, b_rows * b_cols);
    let c_slice = slice::from_raw_parts_mut(c_data, a_rows * b_cols);

    crate::simd::neon::neon_gemm_f32(a_rows, b_cols, a_cols, 1.0, a_slice, b_slice, 0.0, c_slice);

    SciFfiError::Success
}

// ============================================================================
// Activation Functions
// ============================================================================

/// ReLU activation: out = max(0, x)
#[no_mangle]
pub unsafe extern "C" fn scirs2_relu(
    x: *const c_float,
    out: *mut c_float,
    len: usize,
) -> SciFfiError {
    if x.is_null() || out.is_null() {
        return SciFfiError::NullPointer;
    }

    let x_slice = slice::from_raw_parts(x, len);
    let out_slice = slice::from_raw_parts_mut(out, len);

    crate::simd::neon::neon_relu_f32(x_slice, out_slice);

    SciFfiError::Success
}

/// Sigmoid activation
#[no_mangle]
pub unsafe extern "C" fn scirs2_sigmoid(
    x: *const c_float,
    out: *mut c_float,
    len: usize,
) -> SciFfiError {
    if x.is_null() || out.is_null() {
        return SciFfiError::NullPointer;
    }

    let x_slice = slice::from_raw_parts(x, len);
    let out_slice = slice::from_raw_parts_mut(out, len);

    crate::simd::neon::neon_sigmoid_f32(x_slice, out_slice);

    SciFfiError::Success
}

// ============================================================================
// Mobile-Specific Optimizations
// ============================================================================

/// Set battery optimization mode
#[repr(C)]
pub enum SciBatteryMode {
    Performance = 0,
    Balanced = 1,
    PowerSaver = 2,
}

/// Set thermal state
#[repr(C)]
pub enum SciThermalState {
    Normal = 0,
    Warm = 1,
    Hot = 2,
    Critical = 3,
}

/// Global mobile optimizer (thread-local)
thread_local! {
    static MOBILE_OPTIMIZER: std::cell::RefCell<crate::simd::neon::MobileOptimizer> =
        std::cell::RefCell::new(crate::simd::neon::MobileOptimizer::new());
}

/// Configure battery optimization mode
#[no_mangle]
pub unsafe extern "C" fn scirs2_set_battery_mode(mode: SciBatteryMode) -> SciFfiError {
    let battery_mode = match mode {
        SciBatteryMode::Performance => crate::simd::neon::BatteryMode::Performance,
        SciBatteryMode::Balanced => crate::simd::neon::BatteryMode::Balanced,
        SciBatteryMode::PowerSaver => crate::simd::neon::BatteryMode::PowerSaver,
    };

    MOBILE_OPTIMIZER.with(|opt| {
        opt.borrow_mut().battery_mode = battery_mode;
    });

    SciFfiError::Success
}

/// Update thermal state
#[no_mangle]
pub unsafe extern "C" fn scirs2_set_thermal_state(state: SciThermalState) -> SciFfiError {
    let thermal_state = match state {
        SciThermalState::Normal => crate::simd::neon::ThermalState::Normal,
        SciThermalState::Warm => crate::simd::neon::ThermalState::Warm,
        SciThermalState::Hot => crate::simd::neon::ThermalState::Hot,
        SciThermalState::Critical => crate::simd::neon::ThermalState::Critical,
    };

    MOBILE_OPTIMIZER.with(|opt| {
        opt.borrow_mut().thermal_state = thermal_state;
    });

    SciFfiError::Success
}

/// Battery-optimized dot product
#[no_mangle]
pub unsafe extern "C" fn scirs2_vector_dot_battery_optimized(
    a: *const c_float,
    b: *const c_float,
    len: usize,
    result: *mut c_float,
) -> SciFfiError {
    if a.is_null() || b.is_null() || result.is_null() {
        return SciFfiError::NullPointer;
    }

    let a_slice = slice::from_raw_parts(a, len);
    let b_slice = slice::from_raw_parts(b, len);

    let dot = MOBILE_OPTIMIZER
        .with(|opt| crate::simd::neon::neon_dot_battery_optimized(a_slice, b_slice, &opt.borrow()));

    *result = dot;

    SciFfiError::Success
}

// ============================================================================
// Version Information
// ============================================================================

/// Get SciRS2 version string
#[no_mangle]
pub unsafe extern "C" fn scirs2_version() -> *const c_char {
    "0.2.0-mobile\0".as_ptr() as *const c_char
}

/// Check if NEON is available
#[no_mangle]
pub unsafe extern "C" fn scirs2_has_neon() -> c_int {
    if crate::simd::neon::is_neon_available() {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_vector_operations() {
        unsafe {
            assert_eq!(scirs2_init(), SciFfiError::Success);

            // Test vector creation
            let vec = scirs2_vector_create(10);
            assert!(!vec.is_null());

            scirs2_vector_destroy(vec);
            scirs2_shutdown();
        }
    }

    #[test]
    fn test_ffi_matrix_operations() {
        unsafe {
            assert_eq!(scirs2_init(), SciFfiError::Success);

            let mat = scirs2_matrix_create(3, 4);
            assert!(!mat.is_null());

            scirs2_matrix_destroy(mat);
            scirs2_shutdown();
        }
    }
}
