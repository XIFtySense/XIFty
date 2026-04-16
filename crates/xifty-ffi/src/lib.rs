use std::ffi::{CStr, c_char};
use std::mem::ManuallyDrop;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use xifty_core::{ViewMode, XiftyError};
use xifty_json::{to_json_analysis, to_json_probe};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XiftyStatusCode {
    Success = 0,
    InvalidArgument = 1,
    IoError = 2,
    UnsupportedFormat = 3,
    ParseError = 4,
    InternalError = 5,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XiftyViewMode {
    Full = 0,
    Raw = 1,
    Interpreted = 2,
    Normalized = 3,
    Report = 4,
}

impl From<XiftyViewMode> for ViewMode {
    fn from(value: XiftyViewMode) -> Self {
        match value {
            XiftyViewMode::Full => ViewMode::Full,
            XiftyViewMode::Raw => ViewMode::Raw,
            XiftyViewMode::Interpreted => ViewMode::Interpreted,
            XiftyViewMode::Normalized => ViewMode::Normalized,
            XiftyViewMode::Report => ViewMode::Report,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XiftyBuffer {
    pub ptr: *mut u8,
    pub len: usize,
    pub capacity: usize,
}

impl XiftyBuffer {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            capacity: 0,
        }
    }

    fn from_string(value: String) -> Self {
        let bytes = ManuallyDrop::new(value.into_bytes());
        Self {
            ptr: bytes.as_ptr() as *mut u8,
            len: bytes.len(),
            capacity: bytes.capacity(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct XiftyResult {
    pub status: XiftyStatusCode,
    pub output: XiftyBuffer,
    pub error_message: XiftyBuffer,
}

impl XiftyResult {
    pub const fn success(output: XiftyBuffer) -> Self {
        Self {
            status: XiftyStatusCode::Success,
            output,
            error_message: XiftyBuffer::empty(),
        }
    }

    pub fn error(status: XiftyStatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            output: XiftyBuffer::empty(),
            error_message: XiftyBuffer::from_string(message.into()),
        }
    }
}

const VERSION_BYTES: &[u8] = concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes();

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xifty_probe_json(path: *const c_char) -> XiftyResult {
    ffi_boundary(|| {
        let path = path_from_c(path)?;
        let output = xifty_cli::probe_path(path).map_err(map_error)?;
        let json = to_json_probe(&output).map_err(|error| {
            XiftyResult::error(
                XiftyStatusCode::InternalError,
                format!("json serialization failed: {error}"),
            )
        })?;
        Ok(XiftyResult::success(XiftyBuffer::from_string(json)))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xifty_extract_json(
    path: *const c_char,
    view_mode: XiftyViewMode,
) -> XiftyResult {
    ffi_boundary(|| {
        let path = path_from_c(path)?;
        let output = xifty_cli::extract_path(path, view_mode.into()).map_err(map_error)?;
        let json = to_json_analysis(&output).map_err(|error| {
            XiftyResult::error(
                XiftyStatusCode::InternalError,
                format!("json serialization failed: {error}"),
            )
        })?;
        Ok(XiftyResult::success(XiftyBuffer::from_string(json)))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xifty_free_buffer(buffer: XiftyBuffer) {
    if buffer.ptr.is_null() || buffer.capacity == 0 {
        return;
    }

    // SAFETY: The buffer was allocated by XIFty from a Vec<u8> and is only
    // ever released through this ABI function with the original length/capacity.
    unsafe {
        drop(Vec::from_raw_parts(buffer.ptr, buffer.len, buffer.capacity));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn xifty_version() -> *const c_char {
    VERSION_BYTES.as_ptr() as *const c_char
}

fn ffi_boundary<F>(operation: F) -> XiftyResult
where
    F: FnOnce() -> Result<XiftyResult, XiftyResult>,
{
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => error,
        Err(_) => XiftyResult::error(
            XiftyStatusCode::InternalError,
            "xifty internal panic crossed the ABI boundary",
        ),
    }
}

fn path_from_c(path: *const c_char) -> Result<PathBuf, XiftyResult> {
    if path.is_null() {
        return Err(XiftyResult::error(
            XiftyStatusCode::InvalidArgument,
            "path pointer must not be null",
        ));
    }

    // SAFETY: The caller promises a valid NUL-terminated C string.
    let path = unsafe { CStr::from_ptr(path) };
    let path = path.to_str().map_err(|_| {
        XiftyResult::error(XiftyStatusCode::InvalidArgument, "path must be valid UTF-8")
    })?;

    if path.is_empty() {
        return Err(XiftyResult::error(
            XiftyStatusCode::InvalidArgument,
            "path must not be empty",
        ));
    }

    Ok(PathBuf::from(path))
}

fn map_error(error: XiftyError) -> XiftyResult {
    match error {
        XiftyError::Io(error) => XiftyResult::error(XiftyStatusCode::IoError, error.to_string()),
        XiftyError::UnsupportedFormat => {
            XiftyResult::error(XiftyStatusCode::UnsupportedFormat, "unsupported format")
        }
        XiftyError::Parse { message } => XiftyResult::error(XiftyStatusCode::ParseError, message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};
    use std::path::Path;

    fn fixture_path(name: &str) -> CString {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/minimal")
            .join(name);
        CString::new(path.to_string_lossy().as_bytes()).unwrap()
    }

    fn buffer_to_string(buffer: XiftyBuffer) -> String {
        let owned = if buffer.ptr.is_null() {
            String::new()
        } else {
            // SAFETY: Tests only call this on buffers produced by XIFty and do
            // not free them before conversion.
            let bytes = unsafe { std::slice::from_raw_parts(buffer.ptr, buffer.len) };
            String::from_utf8(bytes.to_vec()).unwrap()
        };
        // SAFETY: The buffer was allocated by XIFty and has not yet been freed.
        unsafe { xifty_free_buffer(buffer) };
        owned
    }

    fn error_message(result: XiftyResult) -> String {
        let message = buffer_to_string(result.error_message);
        if !result.output.ptr.is_null() {
            // SAFETY: Defensive cleanup for tests if a failing result ever
            // carries output in the future.
            unsafe { xifty_free_buffer(result.output) };
        }
        message
    }

    #[test]
    fn probe_json_returns_success_for_checked_in_fixture() {
        let path = fixture_path("happy.jpg");
        let result = unsafe { xifty_probe_json(path.as_ptr()) };

        assert_eq!(result.status, XiftyStatusCode::Success);
        let json = buffer_to_string(result.output);
        assert!(json.contains("\"detected_format\": \"jpeg\""));
        assert!(json.contains("\"schema_version\": \"0.1.0\""));
        let error = buffer_to_string(result.error_message);
        assert!(error.is_empty());
    }

    #[test]
    fn extract_json_supports_normalized_view() {
        let path = fixture_path("happy.jpg");
        let result = unsafe { xifty_extract_json(path.as_ptr(), XiftyViewMode::Normalized) };

        assert_eq!(result.status, XiftyStatusCode::Success);
        let json = buffer_to_string(result.output);
        assert!(json.contains("\"normalized\""));
        assert!(json.contains("\"device.make\""));
        let error = buffer_to_string(result.error_message);
        assert!(error.is_empty());
    }

    #[test]
    fn null_path_is_rejected() {
        let result = unsafe { xifty_probe_json(std::ptr::null()) };

        assert_eq!(result.status, XiftyStatusCode::InvalidArgument);
        assert_eq!(error_message(result), "path pointer must not be null");
    }

    #[test]
    fn invalid_utf8_path_is_rejected() {
        let bytes = [0xFF_u8, 0];
        let ptr = bytes.as_ptr() as *const c_char;

        let result = unsafe { xifty_probe_json(ptr) };

        assert_eq!(result.status, XiftyStatusCode::InvalidArgument);
        assert_eq!(error_message(result), "path must be valid UTF-8");
    }

    #[test]
    fn missing_file_maps_to_io_error() {
        let path =
            CString::new("/Users/k/Projects/XIFty/fixtures/minimal/does-not-exist.jpg").unwrap();
        let result = unsafe { xifty_probe_json(path.as_ptr()) };

        assert_eq!(result.status, XiftyStatusCode::IoError);
        assert!(error_message(result).contains("No such file"));
    }

    #[test]
    fn version_returns_non_empty_static_c_string() {
        let ptr = unsafe { xifty_version() };
        assert!(!ptr.is_null());

        let version = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn panics_are_converted_to_internal_errors() {
        let result = ffi_boundary(|| -> Result<XiftyResult, XiftyResult> {
            panic!("boom");
        });

        assert_eq!(result.status, XiftyStatusCode::InternalError);
        assert_eq!(
            error_message(result),
            "xifty internal panic crossed the ABI boundary"
        );
    }

    #[test]
    fn error_mapping_is_stable_for_core_error_categories() {
        assert_eq!(
            map_error(XiftyError::UnsupportedFormat).status,
            XiftyStatusCode::UnsupportedFormat
        );
        assert_eq!(
            map_error(XiftyError::Parse {
                message: "bad structure".into(),
            })
            .status,
            XiftyStatusCode::ParseError
        );
    }
}
