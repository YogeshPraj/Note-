use super::*;

#[test]
fn session_round_trips() {
    let s = Session {
        tabs: vec![SessionTab {
            title: "new 1".into(),
            path: None,
            text: Some("héllo\nworld".into()),
            caret: 3,
            top: 1,
            enc: "utf8".into(),
            eol: "lf".into(),
            encrypted: false,
            pinned: false,
            color: None,
            kind: "text".into(),
            lang: Some("JSON".into()),
        }],
        active: 0,
        wrap: true,
        dark: false,
        minimap: true,
        recent: vec!["C:\\a\\b.txt".into()],
        workspace: Some("C:\\proj".into()),
        search_engine: 2,
        symbols: crate::view::Symbols::default(),
        date_format: "yyyy".into(),
        kept: Vec::new(),
    };
    let p = std::env::temp_dir().join("notepp_session_test.json");
    s.save(&p).unwrap();
    assert_eq!(Session::load(&p), Some(s));
    let _ = std::fs::remove_file(&p);
}

#[test]
fn an_older_snapshot_never_overwrites_a_newer_one() {
    // The autosave writes on a worker thread; closing the window writes at once. Whichever comes last, the newest
    // snapshot is what stays on disk.
    let p = std::env::temp_dir().join(format!("notepp_session_order_{}.json", std::process::id()));
    let with = |marker: &str| Session { date_format: marker.into(), ..Session::default() };
    let (older, newer) = (ticket(), ticket());
    with("newer").save_in_order(&p, newer).unwrap();
    with("older").save_in_order(&p, older).unwrap();
    assert_eq!(Session::load(&p).map(|s| s.date_format), Some("newer".into()));
    let _ = std::fs::remove_file(&p);
}

#[test]
fn tabs_this_app_cannot_show_are_written_back_unchanged() {
    // An unsaved diagram of the older app must survive a load and a save here, content and all.
    let json = r#"{"tabs":[
            {"id":1,"name":"a.txt","filePath":null,"content":"hi","language":"plaintext","active":true,"dirty":true},
            {"id":2,"name":"flow.drawio","filePath":null,"content":"<mxfile>unsaved</mxfile>","language":"drawio","type":"drawio","active":false,"dirty":true}
        ]}"#;
    let p = std::env::temp_dir().join(format!("notepp_session_kept_{}.json", std::process::id()));
    std::fs::write(&p, json).unwrap();
    let s = Session::load(&p).unwrap();
    assert_eq!((s.tabs.len(), s.kept.len()), (1, 1));
    s.save(&p).unwrap();
    let back: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let drawio = &back["tabs"][1];
    assert_eq!((drawio["type"].as_str(), drawio["content"].as_str()), (Some("drawio"), Some("<mxfile>unsaved</mxfile>")));
    assert_eq!(Session::load(&p).unwrap().kept.len(), 1, "it is kept again on the next load");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn reads_a_session_written_by_the_previous_version_of_the_app() {
    let json = r##"{"tabs":[
            {"id":1,"name":"a.json","filePath":"C:\\x\\a.json","content":"{}","language":"json","encoding":"UTF-8","eol":"Windows (CR LF)","active":false,"dirty":false,"pinned":true,"color":"#4488dd"},
            {"id":2,"name":"new 1","filePath":null,"content":"hello","language":"plaintext","encoding":"UTF-16 LE","eol":"Unix (LF)","active":true,"dirty":true,"pinned":false,"color":null},
            {"id":3,"name":"board","filePath":null,"content":"{}","language":"whiteboard","type":"whiteboard","encoding":"UTF-8","eol":"Windows (CR LF)","active":false,"dirty":true},
            {"id":4,"name":"d.drawio","filePath":null,"content":"<x/>","language":"drawio","type":"drawio","active":false}
        ]}"##;
    let p = std::env::temp_dir().join("notepp_session_old.json");
    std::fs::write(&p, json).unwrap();
    let s = Session::load(&p).unwrap();
    let _ = std::fs::remove_file(&p);
    assert_eq!(s.tabs.len(), 3, "the diagram tab has no counterpart");
    assert_eq!(s.active, 1);
    assert_eq!(s.tabs[0].text.as_deref(), Some("{}"), "a file that is gone keeps the stored text");
    let here =
        serde_json::json!({"tabs":[{"name":"Cargo.toml","filePath":concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),"content":"stale","dirty":false}]});
    assert_eq!(tab_from_json(&here["tabs"][0]).unwrap().text, None, "a clean file is read from disk");
    assert!(s.tabs[0].pinned && s.tabs[0].color == Some(crate::workspace::TAB_COLORS[5].1));
    assert_eq!((s.tabs[1].text.as_deref(), s.tabs[1].enc.as_str(), s.tabs[1].eol.as_str()), (Some("hello"), "utf16le", "lf"));
    assert_eq!(s.tabs[2].kind, "whiteboard");
}

#[test]
fn what_is_written_keeps_the_older_shape() {
    let t = SessionTab {
        title: "n.py".into(),
        path: Some("C:\\n.py".into()),
        text: Some("x".into()),
        caret: 1,
        top: 0,
        enc: "utf8bom".into(),
        eol: "cr".into(),
        pinned: false,
        color: Some(crate::workspace::TAB_COLORS[0].1),
        encrypted: false,
        kind: "text".into(),
        lang: None,
    };
    let v = tab_to_json(0, &t, true);
    assert_eq!((v["id"].as_u64(), v["name"].as_str(), v["filePath"].as_str(), v["content"].as_str()), (Some(1), Some("n.py"), Some("C:\\n.py"), Some("x")));
    assert_eq!(
        (v["language"].as_str(), v["encoding"].as_str(), v["eol"].as_str(), v["color"].as_str(), v["active"].as_bool(), v["dirty"].as_bool()),
        (Some("python"), Some("UTF-8 BOM"), Some("Macintosh (CR)"), Some("#E05A5A"), Some(true), Some(true))
    );
    let back = tab_from_json(&v).unwrap();
    assert_eq!(back, t);
    let enc = SessionTab { encrypted: true, text: Some("{\"env\":1}".into()), ..t };
    let v = tab_to_json(0, &enc, false);
    assert!(v["content"].is_null() && v["encrypted"] == true, "protected text never goes into `content`");
    assert_eq!(tab_from_json(&v).unwrap(), enc);
}

#[test]
fn corrupt_session_is_ignored() {
    let p = std::env::temp_dir().join("notepp_session_bad.json");
    std::fs::write(&p, "{ not json").unwrap();
    assert_eq!(Session::load(&p), None);
    let _ = std::fs::remove_file(&p);
}
