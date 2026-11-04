//! Where an embedded page may go on its own. The panes show documents, not the web: the address a page was given by
//! the application is the one exception, and everything else a page tries is refused. No toolkit in here.

/// May a page move the view to `url` without the application asking? Only to what is not a place: the empty page, and
/// the in-memory pages the application sets.
pub fn stays_in_app(url: &str) -> bool {
    let scheme = url.trim_start().split(':').next().unwrap_or_default();
    ["about", "data", "blob"].iter().any(|s| s.eq_ignore_ascii_case(scheme)) && url.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_cannot_leave_for_anywhere() {
        for ok in ["about:blank", "about:srcdoc", "ABOUT:blank", "data:text/html;base64,AA==", "blob:https://x/1"] {
            assert!(stays_in_app(ok), "{ok}");
        }
        for bad in ["https://evil.test/", "http://a", "file:///C:/x.html", "\\\\host\\share\\x.html", "ms-msdt:/id", "search-ms:q", "about", ""] {
            assert!(!stays_in_app(bad), "{bad}");
        }
    }
}
