#![allow(dead_code)]

use std::os::raw::c_char;

#[repr(C)]
pub struct XiftyBuffer {
    pub ptr: *mut c_char,
    pub len: usize,
}

pub fn placeholder_buffer() -> XiftyBuffer {
    XiftyBuffer {
        ptr: std::ptr::null_mut(),
        len: 0,
    }
}
