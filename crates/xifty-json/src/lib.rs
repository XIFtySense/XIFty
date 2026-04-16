use xifty_core::{AnalysisOutput, ProbeOutput};

pub fn to_json_analysis(output: &AnalysisOutput) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(output)
}

pub fn to_json_probe(output: &ProbeOutput) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(output)
}
