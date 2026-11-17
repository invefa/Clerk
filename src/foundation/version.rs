//! Build identity, independent of workspace discovery and runtime Git.

pub fn render(json: bool) -> String {
    use crate::config::schema::{object, quote};
    let commit = option_env!("CLERK_BUILD_COMMIT");
    let dirty = match option_env!("CLERK_BUILD_DIRTY") {
        Some("true") => "true",
        Some("false") => "false",
        _ => "null",
    };
    if json {
        object([
            ("version", quote(env!("CARGO_PKG_VERSION"))),
            ("commit", commit.map(quote).unwrap_or_else(|| "null".into())),
            ("dirty", dirty.into()),
            (
                "formats",
                object([
                    ("workspace", "1".into()),
                    ("workflow", "1".into()),
                    ("batch_draft", "1".into()),
                    ("context", "1".into()),
                    ("log_entry", "1".into()),
                    ("session", "1".into()),
                ]),
            ),
        ]) + "\n"
    } else {
        format!(
            "clerk {} ({}{})\n",
            env!("CARGO_PKG_VERSION"),
            commit.unwrap_or("unknown"),
            if dirty == "true" { ", dirty" } else { "" }
        )
    }
}
