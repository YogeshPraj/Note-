use super::*;

const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyLCJleHAiOjE1MTYyNDI2MjJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

#[test]
fn a_bare_token_is_read() {
    let t = parse(TOKEN).unwrap();
    assert_eq!(t.header["alg"], "HS256");
    assert_eq!(t.payload["name"], "John Doe");
    assert_eq!(t.signature, "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c");
}

#[test]
fn bearer_forms_and_quotes_are_allowed() {
    for text in [format!("Bearer {TOKEN}"), format!("Authorization: Bearer {TOKEN}\n"), format!("\"{TOKEN}\","), format!("  {TOKEN}  ")] {
        assert!(parse(&text).is_some(), "{text}");
    }
}

#[test]
fn other_text_is_not_a_token() {
    let two_words = format!("{TOKEN} and more");
    let extra = format!("{TOKEN}.extra");
    let long = "x".repeat(MAX_LEN + 1);
    for text in ["", "hello world", "a.b.c", "one.two", &two_words, &extra, &long] {
        assert!(parse(text).is_none(), "{text}");
    }
    // A payload that is not an object, and a header without alg.
    assert!(parse("eyJhbGciOiJIUzI1NiJ9.WzFd.x").is_none());
    assert!(parse("e30.e30.x").is_none());
}

#[test]
fn times_are_shown_in_utc() {
    assert_eq!(utc(0).as_deref(), Some("1970-01-01 00:00:00 UTC"));
    assert_eq!(utc(1_516_239_022).as_deref(), Some("2018-01-18 01:30:22 UTC"));
    assert_eq!(utc(-86_400).as_deref(), Some("1969-12-31 00:00:00 UTC"));
    assert_eq!(utc(i64::MAX), None);
    assert_eq!(utc(i64::MIN), None);
    assert_eq!(span(3 * 86_400 + 4 * 3600 + 5), "3 days 4 hours");
    assert_eq!(span(0), "0 seconds");
    assert_eq!(span(61), "1 minute 1 second");
}

#[test]
fn the_page_says_expired_or_not_and_that_the_signature_is_not_checked() {
    let t = parse(TOKEN).unwrap();
    let late = html(&t, 1_516_242_622 + 90_000);
    assert!(late.contains("Expired"), "{late}");
    assert!(late.contains("1 day 1 hour ago"), "{late}");
    assert!(late.contains("John Doe"));
    assert!(late.contains("not checked"));
    assert!(html(&t, 1_516_239_022).contains("Not expired"));
}

#[test]
fn nothing_in_a_token_reaches_the_page_as_markup() {
    let header = r#"{"alg":"none","typ":"<script>"}"#;
    let payload = r#"{"iss":"<img src=x onerror=alert(1)>","exp":9223372036854775807}"#;
    let enc = |s: &str| URL_SAFE_NO_PAD.encode(s);
    let t = parse(&format!("{}.{}.", enc(header), enc(payload))).unwrap();
    let page = html(&t, 0);
    assert!(!page.contains("<script>") && !page.contains("<img"), "{page}");
    assert!(page.contains("Unsecured token"));
    assert!(page.contains("There is none."));
}
