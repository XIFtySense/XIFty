use xifty_core::{ViewMode, XiftyError};

pub fn probe_bytes_json(bytes: &[u8], file_name: Option<&str>) -> Result<String, XiftyError> {
    let output = xifty_cli::probe_bytes(bytes.to_vec(), file_name.map(str::to_owned))?;
    xifty_json::to_json_probe(&output).map_err(json_error)
}

pub fn extract_bytes_json(
    bytes: &[u8],
    file_name: Option<&str>,
    view_mode: Option<&str>,
) -> Result<String, XiftyError> {
    let output = xifty_cli::extract_bytes(
        bytes.to_vec(),
        file_name.map(str::to_owned),
        parse_view_mode(view_mode)?,
    )?;
    xifty_json::to_json_analysis(&output).map_err(json_error)
}

fn parse_view_mode(view_mode: Option<&str>) -> Result<ViewMode, XiftyError> {
    match view_mode.unwrap_or("full") {
        "full" => Ok(ViewMode::Full),
        "raw" => Ok(ViewMode::Raw),
        "interpreted" => Ok(ViewMode::Interpreted),
        "normalized" => Ok(ViewMode::Normalized),
        "report" => Ok(ViewMode::Report),
        other => Err(XiftyError::Parse {
            message: format!("unsupported view mode: {other}"),
        }),
    }
}

fn json_error(error: serde_json::Error) -> XiftyError {
    XiftyError::Parse {
        message: format!("json serialization failed: {error}"),
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn probe_bytes(bytes: &[u8], file_name: Option<String>) -> Result<String, JsValue> {
    probe_bytes_json(bytes, file_name.as_deref()).map_err(error_to_js)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn extract_bytes(
    bytes: &[u8],
    file_name: Option<String>,
    view_mode: Option<String>,
) -> Result<String, JsValue> {
    extract_bytes_json(bytes, file_name.as_deref(), view_mode.as_deref()).map_err(error_to_js)
}

#[cfg(target_arch = "wasm32")]
fn error_to_js(error: XiftyError) -> JsValue {
    JsValue::from_str(&error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const HAPPY_JPEG: &[u8] = include_bytes!("../../../fixtures/minimal/happy.jpg");
    const MALFORMED_JPEG: &[u8] = include_bytes!("../../../fixtures/minimal/malformed_app1.jpg");

    #[test]
    fn probe_bytes_json_preserves_filename_hint() {
        let output = probe_bytes_json(HAPPY_JPEG, Some("browser-happy.jpg")).unwrap();
        let json: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            json["input"]["path"],
            Value::String("browser-happy.jpg".into())
        );
        assert_eq!(
            json["input"]["detected_format"],
            Value::String("jpeg".into())
        );
    }

    #[test]
    fn extract_bytes_json_supports_normalized_view() {
        let output =
            extract_bytes_json(HAPPY_JPEG, Some("browser-happy.jpg"), Some("normalized")).unwrap();
        let json: Value = serde_json::from_str(&output).unwrap();
        assert!(json.get("raw").is_none());
        assert!(json.get("interpreted").is_none());
        assert!(json.get("normalized").is_some());
        assert_eq!(
            json["normalized"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|field| field["field"] == "device.make")
                .unwrap()["value"]["value"],
            Value::String("XIFtyCam".into())
        );
    }

    #[test]
    fn extract_bytes_json_surfaces_report_issues_for_malformed_input() {
        let output =
            extract_bytes_json(MALFORMED_JPEG, Some("broken.jpg"), Some("report")).unwrap();
        let json: Value = serde_json::from_str(&output).unwrap();
        assert!(json["report"]["issues"].as_array().unwrap().len() >= 1);
        assert!(json.get("raw").is_none());
        assert!(json.get("normalized").is_none());
    }

    #[test]
    fn invalid_view_mode_is_rejected() {
        let error = extract_bytes_json(HAPPY_JPEG, None, Some("bogus")).unwrap_err();
        assert!(error.to_string().contains("unsupported view mode"));
    }
}
