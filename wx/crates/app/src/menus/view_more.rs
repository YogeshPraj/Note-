//! The View, Encoding, Language, Settings, Tools and Help menus.

use super::*;

/// The View menu: modes and switches on top, everything else grouped into sub-menus so the menu stays short.
pub(super) fn menu_view_basic() -> M {
    let view = B::popup();
    B::cmd(view, ID_ON_TOP);
    B::cmd(view, ID_FULLSCREEN);
    B::cmd(view, ID_POSTIT);
    B::cmd(view, ID_DISTRACTION);
    B::sep(view);
    B::cmd(view, ID_WRAP);
    B::cmd(view, ID_THEME);
    B::cmd(view, ID_PREVIEW);
    B::cmd(view, ID_MD_TOOLBAR);
    B::sep(view);
    view
}

pub(super) fn view_panels() -> M {
    let panels = B::popup();
    B::cmd(panels, ID_WORKSPACE_TOGGLE);
    B::cmd(panels, ID_MINIMAP);
    B::cmd(panels, ID_DOC_LIST);
    B::cmd(panels, ID_FUNC_LIST);
    panels
}

pub(super) fn view_folding() -> M {
    let folding = B::popup();
    B::cmd(folding, ID_FOLD_ALL);
    B::cmd(folding, ID_UNFOLD_ALL);
    B::cmd(folding, ID_FOLD_CUR);
    B::cmd(folding, ID_UNFOLD_CUR);
    let fold_level = B::popup();
    let unfold_level = B::popup();
    for i in 0..8 {
        B::add(fold_level, ID_FOLD_LEVEL_BASE + i, &format!("Level {}\tAlt+{}", i + 1, i + 1));
        B::add(unfold_level, ID_UNFOLD_LEVEL_BASE + i, &format!("Level {}\tAlt+Shift+{}", i + 1, i + 1));
    }
    B::submenu(folding, "Fold Level", fold_level);
    B::submenu(folding, "Unfold Level", unfold_level);
    B::sep(folding);
    B::cmd(folding, ID_HIDE_LINES);
    folding
}

pub(super) fn view_symbols() -> M {
    let sym = B::popup();
    B::cmd(sym, ID_SYM_WS);
    B::cmd(sym, ID_SYM_EOL);
    B::cmd(sym, ID_SYM_NONPRINT);
    B::cmd(sym, ID_SYM_CTRL);
    B::cmd(sym, ID_SYM_ALL);
    B::sep(sym);
    B::cmd(sym, ID_SYM_INDENT);
    B::cmd(sym, ID_SYM_WRAP);
    sym
}

pub(super) fn view_tabs() -> M {
    let tab = B::popup();
    for i in 0..9 {
        B::add(tab, ID_TAB_BASE + i, &format!("Tab {}", i + 1));
    }
    B::sep(tab);
    B::cmd(tab, ID_NEXT_TAB);
    B::cmd(tab, ID_PREV_TAB);
    B::cmd(tab, ID_MOVE_TAB_FWD);
    B::cmd(tab, ID_MOVE_TAB_BACK);
    tab
}

/// Moving a document to another instance of the program, or opening it in one.
pub(super) fn view_new_instance() -> M {
    let other = B::popup();
    B::cmd(other, ID_MOVE_NEW_INSTANCE);
    B::cmd(other, ID_OPEN_NEW_INSTANCE);
    other
}

/// The sub-menus and the closing commands of the View menu.
pub(super) fn menu_view_rest(view: M) {
    B::submenu(view, "Panels", view_panels());
    B::submenu(view, "Folding", view_folding());
    B::submenu(view, "Show Symbol", view_symbols());
    let zoom = B::popup();
    B::cmd(zoom, ID_ZOOMIN);
    B::cmd(zoom, ID_ZOOMOUT);
    B::cmd(zoom, ID_ZOOMRESET);
    B::submenu(view, "Zoom", zoom);
    B::submenu(view, "Tab", view_tabs());
    let direction = B::popup();
    B::cmd(direction, ID_RTL);
    B::cmd(direction, ID_LTR);
    B::submenu(view, "Text Direction", direction);
    B::submenu(view, "New Instance", view_new_instance());
    B::cmd(view, ID_FOCUS_OTHER);
    B::cmd(view, ID_NEXT_AREA);
    B::cmd(view, ID_PREV_AREA);
    let follow = B::popup();
    B::cmd(follow, ID_MONITOR);
    B::cmd(follow, ID_LOG_FOLLOW);
    B::submenu(view, "Monitoring", follow);
    let browsers = B::popup();
    for (i, (name, _)) in BROWSERS.iter().enumerate() {
        B::add(browsers, ID_BROWSER_BASE + i, name);
    }
    B::submenu(view, "View Current File in", browsers);
    B::sep(view);
    B::cmd(view, ID_SUMMARY);
    B::cmd(view, ID_PALETTE);
    B::cmd(view, ID_QUICKOPEN);
}
/// The Encoding and Language menus.
pub(super) fn menu_encoding_language() -> (M, M) {
    let enc = B::popup();
    add_items(enc, &EncodeInItems);
    let charsets = B::popup();
    add_groups(charsets, &CharsetGroups);
    B::submenu(enc, "Character sets", charsets);
    B::sep(enc);
    add_items(enc, &ConvertToItems);

    // Languages: "Normal Text", then one submenu per initial letter.
    let lang = B::popup();
    add_items(lang, &Languages);
    add_groups(lang, &Languages);
    (enc, lang)
}

/// The ? menu.
pub(super) fn menu_help() -> M {
    let help = B::popup();
    B::cmd(help, ID_REPORT_ISSUE);
    B::cmd(help, ID_ABOUT);
    B::sep(help);
    B::cmd(help, ID_UPDATE_CHECK);
    B::cmd(help, ID_UPDATE_AUTO);
    help
}

/// The Settings menu.
pub(super) fn menu_settings() -> M {
    let settings_menu = B::popup();
    B::cmd(settings_menu, ID_PREFS);
    B::cmd(settings_menu, ID_STYLE_CFG);
    B::cmd(settings_menu, ID_SHORTCUTS);
    B::sep(settings_menu);
    let themes = B::popup();
    B::cmd(themes, ID_PRESET_BASE);
    B::add(themes, ID_PRESET_BASE + 1, "Default (dark)");
    B::sep(themes);
    for (i, p) in settings::PRESETS.iter().enumerate() {
        B::add(themes, ID_PRESET_BASE + 2 + i, p.name);
    }
    B::submenu(settings_menu, "Themes", themes);
    let import = B::popup();
    B::cmd(import, ID_IMPORT_THEME);
    B::cmd(import, ID_EXPORT_THEME);
    B::submenu(settings_menu, "Import", import);
    B::sep(settings_menu);
    B::cmd(settings_menu, ID_ENC_SETTINGS);
    B::cmd(settings_menu, ID_EDIT_CONTEXT);
    settings_menu
}

/// The Tools menu: one submenu per digest, then the text tools.
pub(super) fn menu_tools() -> M {
    let tools = B::popup();
    for (i, algo) in ALGOS.iter().enumerate() {
        let sub = B::popup();
        B::add(sub, ID_HASH_BASE + i * 10, "Generate...");
        B::add(sub, ID_HASH_BASE + i * 10 + 1, "Generate from files...");
        B::add(sub, ID_HASH_BASE + i * 10 + 2, "Generate from selection into clipboard");
        B::submenu(tools, algo.name(), sub);
    }
    B::sep(tools);
    let text_tools = B::popup();
    B::cmd(text_tools, ID_B64_ENC);
    B::cmd(text_tools, ID_B64_DEC);
    B::sep(text_tools);
    B::cmd(text_tools, ID_URL_ENC);
    B::cmd(text_tools, ID_URL_DEC);
    B::sep(text_tools);
    B::cmd(text_tools, ID_JSON_PRETTY);
    B::cmd(text_tools, ID_JSON_MIN);
    B::cmd(text_tools, ID_XML_PRETTY);
    B::cmd(text_tools, ID_XML_MIN);
    B::submenu(tools, "Text Tools", text_tools);
    B::sep(tools);
    B::cmd(tools, ID_PASSWORDS);
    B::cmd(tools, ID_HTTP_SEND);
    tools
}
