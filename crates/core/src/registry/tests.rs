use super::*;

#[test]
fn ids_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for s in SPECS {
        assert!(seen.insert(s.id), "command id {} is registered twice", s.id);
    }
}

#[test]
fn every_default_shortcut_parses() {
    for s in SPECS.iter().filter(|s| !s.shortcut.is_empty()) {
        assert!(crate::shortcuts::parse(s.shortcut).is_some(), "{}: bad shortcut {:?}", s.palette, s.shortcut);
    }
}

#[test]
fn every_typed_command_has_a_spec_and_round_trips() {
    for c in Command::FIXED {
        assert!(c.spec().is_some(), "{c:?} is not registered");
        assert_eq!(Command::from_id(c.id()), Some(c));
    }
}

#[test]
fn tooltip_names_the_command_and_its_shortcut() {
    assert_eq!(Command::FileSave.spec().map(CommandSpec::tooltip).unwrap_or_default(), "Save (Ctrl+S)");
    assert_eq!(Command::FileOpen.spec().map(CommandSpec::tooltip).unwrap_or_default(), "Open (Ctrl+O)");
}

#[test]
fn menu_text_uses_the_override_when_given() {
    let s = Command::FileNew.spec().expect("registered");
    assert_eq!(s.menu_text(None), "&New\tCtrl+N");
    assert_eq!(s.menu_text(Some("Ctrl+Alt+N")), "&New\tCtrl+Alt+N");
    assert_eq!(s.menu_text(Some("")), "&New");
}

#[test]
fn text_commands_do_not_run_on_a_whiteboard() {
    // On a whiteboard the editor still holds the last text tab's document: editing it would change a hidden tab.
    let wb = CommandState { whiteboard: true, has_selection: true, can_undo: true, ..CommandState::default() };
    for id in [ID_UNDO, ID_PASTE, ID_CUT, ID_L_SORT_ASC, ID_C_UPPER, ID_EOL_LF, ID_FIND, ID_REPLACE, ID_FINDNEXT, ID_GOTO, ID_BM_TOGGLE] {
        assert!(!is_enabled(id, wb), "{id} ran on a whiteboard");
        assert!(is_enabled(id, CommandState { whiteboard: false, ..wb }), "{id} is blocked on a text tab");
    }
    for id in [ID_ENCODE_BASE, ID_CHARSET_BASE + 3, ID_CONVERT_BASE + 1, ID_LANG_BASE + 40, ID_UNFOLD_LEVEL_BASE + 7, ID_JSON_PRETTY, ID_HASH_BASE + 12] {
        assert!(!is_enabled(id, wb), "{id} ran on a whiteboard");
    }
}

#[test]
fn file_window_and_tool_commands_still_run_on_a_whiteboard() {
    let wb = CommandState { whiteboard: true, has_path: true, ..CommandState::default() };
    for id in
        [ID_SAVE, ID_SAVEAS, ID_CLOSE, ID_NEW, ID_FIF, ID_NEXT_RESULT, ID_CP_FULLPATH, ID_CLIP_HISTORY, ID_ENGINE_BASE + 2, ID_HASH_BASE, ID_HASH_BASE + 11]
    {
        assert!(is_enabled(id, wb), "{id} is blocked on a whiteboard");
    }
    for id in [ID_THEME, ID_PREVIEW, ID_PREFS, ID_WINDOWS, ID_ABOUT, ID_NEXT_TAB, ID_TAB_PIN] {
        assert!(!needs_text(id), "{id} should not need text");
    }
}

#[test]
fn selection_commands_wait_for_a_selection() {
    let none = CommandState::default();
    let some = CommandState { has_selection: true, ..CommandState::default() };
    assert!(!is_enabled(ID_COPY, none));
    assert!(is_enabled(ID_COPY, some));
    assert!(is_enabled(ID_NEW, none));
}

#[test]
fn undo_and_redo_follow_their_stacks() {
    let s = CommandState { can_undo: true, ..CommandState::default() };
    assert!(is_enabled(ID_UNDO, s));
    assert!(!is_enabled(ID_REDO, s));
}

#[test]
fn family_members_decode_to_typed_commands_and_back() {
    for id in [
        ID_RECENT_BASE,
        ID_RECENT_BASE + 7,
        ID_ENCODE_BASE + 4,
        ID_CONVERT_BASE + 2,
        ID_CHARSET_BASE + crate::doc::CHARSETS.len(),
        ID_LANG_BASE,
        ID_LANG_BASE + 5,
    ] {
        let c = Command::from_id(id).expect("a family member");
        assert_eq!(c.id(), id);
    }
    assert_eq!(Command::from_id(ID_RECENT_BASE + 3), Some(Command::OpenRecent(3)));
    assert_eq!(Command::from_id(ID_LANG_BASE + 1), Some(Command::SetLanguage(1)));
}

#[test]
fn ids_outside_every_family_are_not_commands() {
    assert_eq!(Command::from_id(ID_RECENT_BASE + RECENT_SLOTS), None);
    assert_eq!(Command::from_id(ID_ENCODE_BASE + crate::tables::ENCODE_IN.len()), None);
    assert_eq!(Command::from_id(ID_LANG_BASE + crate::lang::LANGUAGES.len()), None);
}

#[test]
fn palette_lists_only_commands_with_a_label() {
    assert!(palette().all(|s| !s.palette.is_empty()));
    assert!(palette().count() > 100);
}
