use xifty_core::{Issue, MetadataEntry, Report};

pub fn build_report(mut issues: Vec<Issue>, entries: &[MetadataEntry]) -> Report {
    if entries.is_empty() {
        issues.push(xifty_core::Issue {
            severity: xifty_core::Severity::Info,
            code: "no_metadata_entries".into(),
            message: "no supported metadata entries were decoded".into(),
            offset: None,
            context: None,
        });
    }
    Report {
        issues,
        conflicts: Vec::new(),
    }
}
