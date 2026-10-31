//! The command catalogue shared by every shell: command ids (the numbers are also stored in saved
//! shortcut maps), the Command Palette table and the default shortcuts.
//! Shells decide how a command looks and how it runs; the numbering and wording are common.

pub const ID_PREVIEW: usize = 5301;
pub const ID_MD_BOLD: usize = 5400;
pub const ID_MD_ITALIC: usize = 5401;
pub const ID_MD_STRIKE: usize = 5402;
pub const ID_MD_CODE: usize = 5403;
pub const ID_MD_H1: usize = 5404;
pub const ID_MD_H2: usize = 5405;
pub const ID_MD_H3: usize = 5406;
pub const ID_MD_QUOTE: usize = 5407;
pub const ID_MD_BULLET: usize = 5408;
pub const ID_MD_NUMBERED: usize = 5409;
pub const ID_MD_TASK: usize = 5410;
pub const ID_MD_LINK: usize = 5411;
pub const ID_MD_IMAGE: usize = 5412;
pub const ID_MD_TABLE: usize = 5413;
pub const ID_MD_CODEBLOCK: usize = 5414;
pub const ID_MD_RULE: usize = 5415;
pub const ID_MD_TOOLBAR: usize = 5417;
pub const ID_MD_H4: usize = 5418;
pub const ID_MD_H5: usize = 5419;
pub const ID_MD_H6: usize = 5420;
pub const ID_NEW_WB: usize = 1008;
pub const ID_NEW: usize = 1001;
pub const ID_OPEN: usize = 1002;
pub const ID_SAVE: usize = 1003;
pub const ID_SAVEAS: usize = 1004;
pub const ID_SAVEALL: usize = 1005;
pub const ID_CLOSE: usize = 1006;
pub const ID_EXIT: usize = 1007;
pub const ID_UNDO: usize = 2001;
pub const ID_REDO: usize = 2002;
pub const ID_CUT: usize = 2003;
pub const ID_COPY: usize = 2004;
pub const ID_PASTE: usize = 2005;
pub const ID_SELALL: usize = 2006;
pub const ID_EOL_CRLF: usize = 2010;
pub const ID_EOL_LF: usize = 2011;
pub const ID_EOL_CR: usize = 2012;
pub const ID_WRAP: usize = 3001;
pub const ID_ZOOMIN: usize = 3002;
pub const ID_ZOOMOUT: usize = 3003;
pub const ID_ZOOMRESET: usize = 3004;
pub const ID_THEME: usize = 3005;
pub const ID_MINIMAP: usize = 3006;
pub const ID_NEXT_TAB: usize = 4001;
pub const ID_PREV_TAB: usize = 4002;
pub const ID_FIND: usize = 5001;
pub const ID_REPLACE: usize = 5002;
pub const ID_FINDNEXT: usize = 5003;
pub const ID_FINDPREV: usize = 5004;
pub const ID_GOTO: usize = 5005;
pub const ID_FIF: usize = 5006;
pub const ID_MARK: usize = 5007;
pub const ID_PALETTE: usize = 5201;
pub const ID_QUICKOPEN: usize = 5202;
pub const ID_BM_TOGGLE: usize = 5101;
pub const ID_BM_NEXT: usize = 5102;
pub const ID_BM_PREV: usize = 5103;
pub const ID_BM_CLEAR: usize = 5104;
pub const ID_BM_CUT: usize = 5105;
pub const ID_BM_COPY: usize = 5106;
pub const ID_BM_PASTE: usize = 5107;
pub const ID_BM_DELETE: usize = 5108;
pub const ID_BM_DELETE_UNMARKED: usize = 5109;
pub const ID_BM_INVERT: usize = 5110;
pub const ID_SEL_FIND_NEXT: usize = 5011;
pub const ID_SEL_FIND_PREV: usize = 5012;
pub const ID_VOL_NEXT: usize = 5013;
pub const ID_VOL_PREV: usize = 5014;
pub const ID_INCR: usize = 5015;
pub const ID_RESULTS: usize = 5016;
pub const ID_NEXT_RESULT: usize = 5017;
pub const ID_PREV_RESULT: usize = 5018;
pub const ID_BRACE_GOTO: usize = 5019;
pub const ID_BRACE_SEL: usize = 5020;
pub const ID_FIND_CHARS: usize = 5021;
pub const ID_STYLE_ALL_BASE: usize = 5030; // + style slot 0..4
pub const ID_STYLE_ONE_BASE: usize = 5040;
pub const ID_CLEAR_STYLE_BASE: usize = 5050; // + 0..4, + 5 = all styles
pub const ID_JUMP_UP_BASE: usize = 5060;
pub const ID_JUMP_DOWN_BASE: usize = 5070;
pub const ID_COPY_STYLED_BASE: usize = 5080; // + 0..4, + 5 = all styles
pub const ID_ON_TOP: usize = 3101;
pub const ID_FULLSCREEN: usize = 3102;
pub const ID_POSTIT: usize = 3103;
pub const ID_DISTRACTION: usize = 3104;
pub const ID_BROWSER_BASE: usize = 3110; // + index into BROWSERS
pub const ID_SYM_WS: usize = 3120;
pub const ID_SYM_EOL: usize = 3121;
pub const ID_SYM_NONPRINT: usize = 3122;
pub const ID_SYM_CTRL: usize = 3123;
pub const ID_SYM_ALL: usize = 3124;
pub const ID_SYM_INDENT: usize = 3125;
pub const ID_SYM_WRAP: usize = 3126;
pub const ID_MOVE_NEW_INSTANCE: usize = 3130;
pub const ID_OPEN_NEW_INSTANCE: usize = 3131;
pub const ID_MOVE_OTHER_VIEW: usize = 3132;
pub const ID_CLONE_OTHER_VIEW: usize = 3133;
pub const ID_TAB_BASE: usize = 3140; // + 0..8 = Tab 1..9
pub const ID_MOVE_TAB_FWD: usize = 3150;
pub const ID_MOVE_TAB_BACK: usize = 3151;
pub const ID_FOCUS_OTHER: usize = 3160;
pub const ID_HIDE_LINES: usize = 3161;
pub const ID_FOLD_ALL: usize = 3170;
pub const ID_UNFOLD_ALL: usize = 3171;
pub const ID_FOLD_CUR: usize = 3172;
pub const ID_UNFOLD_CUR: usize = 3173;
pub const ID_FOLD_LEVEL_BASE: usize = 3180; // + 0..7
pub const ID_UNFOLD_LEVEL_BASE: usize = 3190; // + 0..7
pub const ID_SUMMARY: usize = 3200;
pub const ID_PROJECT_BASE: usize = 3201; // + 0..2 (not available)
pub const ID_DOC_LIST: usize = 3206;
pub const ID_FUNC_LIST: usize = 3207;
pub const ID_SYNC_V: usize = 3208;
pub const ID_SYNC_H: usize = 3209;
pub const ID_RTL: usize = 3210;
pub const ID_LTR: usize = 3211;
pub const ID_MONITOR: usize = 3212;
pub const ID_PREFS: usize = 3301;
pub const ID_STYLE_CFG: usize = 3302;
pub const ID_SHORTCUTS: usize = 3303;
pub const ID_IMPORT_THEME: usize = 3305;
pub const ID_EXPORT_THEME: usize = 3306;
pub const ID_EDIT_CONTEXT: usize = 3307;
pub const ID_PRESET_BASE: usize = 3320; // + 0 = default light, 1 = default dark, 2.. = settings::PRESETS
pub const ID_HASH_BASE: usize = 3400; // + algorithm index * 10 + (0 generate, 1 from files, 2 from selection)
pub const ID_B64_ENC: usize = 3450;
pub const ID_B64_DEC: usize = 3451;
pub const ID_URL_ENC: usize = 3452;
pub const ID_URL_DEC: usize = 3453;
pub const ID_JSON_PRETTY: usize = 3454;
pub const ID_JSON_MIN: usize = 3455;
pub const ID_XML_PRETTY: usize = 3456;
pub const ID_XML_MIN: usize = 3457;
pub const ID_WINDOWS: usize = 3600;
pub const ID_WINDOW_SORT_BASE: usize = 3610; // + index into dialogs::SORTS
pub const ID_WINDOW_DOC_BASE: usize = 11000; // + 0..9: the first nine tabs
pub const ID_L_DUP: usize = 2101;
pub const ID_L_DEL: usize = 2102;
pub const ID_L_UP: usize = 2103;
pub const ID_L_DOWN: usize = 2104;
pub const ID_L_SORT_ASC: usize = 2105;
pub const ID_L_SORT_DESC: usize = 2106;
pub const ID_L_DEDUP: usize = 2107;
pub const ID_L_NOEMPTY: usize = 2108;
pub const ID_L_TRIM: usize = 2109;
pub const ID_C_UPPER: usize = 2110;
pub const ID_C_LOWER: usize = 2111;
pub const ID_COMMENT: usize = 2112;
pub const ID_DELETE: usize = 2201;
pub const ID_BEGIN_END_SEL: usize = 2202;
pub const ID_BEGIN_END_COL: usize = 2203;
pub const ID_INS_DATE_SHORT: usize = 2210;
pub const ID_INS_DATE_LONG: usize = 2211;
pub const ID_INS_DATE_CUSTOM: usize = 2212;
pub const ID_CP_FULLPATH: usize = 2220;
pub const ID_CP_NAME: usize = 2221;
pub const ID_CP_DIR: usize = 2222;
pub const ID_CP_ALLNAMES: usize = 2223;
pub const ID_CP_ALLPATHS: usize = 2224;
pub const ID_INDENT_INC: usize = 2230;
pub const ID_INDENT_DEC: usize = 2231;
pub const ID_CASE_PROPER: usize = 2240;
pub const ID_CASE_PROPER_BLEND: usize = 2241;
pub const ID_CASE_SENTENCE: usize = 2242;
pub const ID_CASE_SENTENCE_BLEND: usize = 2243;
pub const ID_CASE_INVERT: usize = 2244;
pub const ID_CASE_RANDOM: usize = 2245;
pub const ID_L_RM_CONSEC: usize = 2250;
pub const ID_L_SPLIT: usize = 2251;
pub const ID_L_JOIN: usize = 2252;
pub const ID_L_RM_BLANK: usize = 2253;
pub const ID_L_INS_ABOVE: usize = 2254;
pub const ID_L_INS_BELOW: usize = 2255;
pub const ID_L_REVERSE: usize = 2256;
pub const ID_L_RANDOM: usize = 2257;
pub const ID_L_SORT_NOCASE_ASC: usize = 2258;
pub const ID_L_SORT_NOCASE_DESC: usize = 2259;
pub const ID_L_SORT_INT_ASC: usize = 2260;
pub const ID_L_SORT_INT_DESC: usize = 2261;
pub const ID_L_SORT_DOT_ASC: usize = 2262;
pub const ID_L_SORT_DOT_DESC: usize = 2263;
pub const ID_L_SORT_COMMA_ASC: usize = 2264;
pub const ID_L_SORT_COMMA_DESC: usize = 2265;
pub const ID_L_CUT: usize = 2266;
pub const ID_L_COPY: usize = 2267;
pub const ID_L_TRANSPOSE: usize = 2268;
pub const ID_CMT_LINE: usize = 2271;
pub const ID_UNCMT_LINE: usize = 2272;
pub const ID_CMT_BLOCK: usize = 2273;
pub const ID_UNCMT_BLOCK: usize = 2274;
pub const ID_AC_FUNC: usize = 2280;
pub const ID_AC_WORD: usize = 2281;
pub const ID_AC_HINT: usize = 2282;
pub const ID_AC_PATH: usize = 2283;
pub const ID_B_TRIM_LEAD: usize = 2291;
pub const ID_B_TRIM_BOTH: usize = 2292;
pub const ID_B_EOL_SPACE: usize = 2293;
pub const ID_B_UNNECESSARY: usize = 2294;
pub const ID_B_TAB_SPACE: usize = 2295;
pub const ID_B_SPACE_TAB_ALL: usize = 2296;
pub const ID_B_SPACE_TAB_LEAD: usize = 2297;
pub const ID_PS_HTML: usize = 2300;
pub const ID_PS_RTF: usize = 2301;
pub const ID_PS_COPY_BIN: usize = 2302;
pub const ID_PS_CUT_BIN: usize = 2303;
pub const ID_PS_PASTE_BIN: usize = 2304;
pub const ID_SEL_OPEN: usize = 2310;
pub const ID_SEL_FOLDER: usize = 2311;
pub const ID_SEL_SEARCH: usize = 2312;
pub const ID_ENGINE_BASE: usize = 2320; // + index into SEARCH_ENGINES
pub const ID_COLUMN_MODE: usize = 2330;
pub const ID_COLUMN_EDITOR: usize = 2331;
pub const ID_CHAR_PANEL: usize = 2332;
pub const ID_CLIP_HISTORY: usize = 2333;
pub const ID_READONLY: usize = 2334;
pub const ID_CLEAR_RO: usize = 2335;
pub const ID_RECENT_BASE: usize = 10000; // + index into the recent list
pub const ID_RECENT_CLEAR: usize = 10100;
pub const ID_OPEN_FOLDER_EXPLORER: usize = 1101;
pub const ID_OPEN_FOLDER_CMD: usize = 1102;
pub const ID_OPEN_FOLDER_WS: usize = 1103;
pub const ID_OPEN_VIEWER: usize = 1104;
pub const ID_OPEN_WORKSPACE: usize = 1105;
pub const ID_RELOAD: usize = 1106;
pub const ID_SAVE_COPY: usize = 1107;
pub const ID_RENAME: usize = 1108;
pub const ID_CLOSE_ALL: usize = 1109;
pub const ID_CLOSE_OTHERS: usize = 1110;
pub const ID_CLOSE_LEFT: usize = 1111;
pub const ID_CLOSE_RIGHT: usize = 1112;
pub const ID_CLOSE_UNCHANGED: usize = 1113;
pub const ID_RECYCLE: usize = 1114;
pub const ID_LOAD_SESSION: usize = 1115;
pub const ID_SAVE_SESSION: usize = 1116;
pub const ID_PRINT: usize = 1117;
pub const ID_PRINT_NOW: usize = 1118;
pub const ID_PAGE_SETUP: usize = 1125;
pub const ID_ENCRYPT: usize = 1126;
pub const ID_DECRYPT: usize = 1127;
pub const ID_ENC_TOGGLE: usize = 1128; // the toolbar's lock button
pub const ID_ENC_SETTINGS: usize = 3340;
pub const ID_PASSWORDS: usize = 3341;
pub const ID_HTTP_SEND: usize = 3342;
pub const ID_PW_LOCK: usize = 3343;
pub const ID_ENC_LOCK: usize = 3344;
pub const ID_PW_TOOLBAR: usize = 3345;
pub const ID_TAB_PIN: usize = 5600;
pub const ID_TAB_COLOR_BASE: usize = 5601; // + 0..7 = the colours
pub const ID_TAB_COLOR_NONE: usize = 5609;
pub const ID_TAB_CMP_FILE: usize = 5610;
pub const ID_TAB_CMP_CLIP: usize = 5611;
pub const ID_TAB_REVEAL: usize = 5612;
pub const ID_LOG_FOLLOW: usize = 3213;
pub const ID_UPDATE_PILL: usize = 3350;
pub const ID_UPDATE_CHECK: usize = 3351;
pub const ID_UPDATE_AUTO: usize = 3352;
pub const ID_RESTORE_CLOSED: usize = 1119;
pub const ID_OPEN_ALL_RECENT: usize = 1120;
pub const ID_WORKSPACE_TOGGLE: usize = 3007;
pub const FILE_OPEN_FOLDER_POS: u32 = 3;
pub const ID_ABOUT: usize = 9001;
pub const ID_ENCODE_BASE: usize = 7000; // + index into ENCODE_IN: "Encode in ..."
pub const ID_CHARSET_BASE: usize = 7100; // + index into CHARSETS (CHARSETS.len() = Windows-1252)
pub const ID_CONVERT_BASE: usize = 7200; // + index into ENCODE_IN: "Convert to ..."
pub const ID_LANG_BASE: usize = 8000; // + index into LANGUAGES

/// Everything the Command Palette offers: (label, command id, shortcut).
pub const PALETTE_COMMANDS: &[(&str, usize, &str)] = &[
    ("File: New", ID_NEW, "Ctrl+N"),
    ("File: New Whiteboard", ID_NEW_WB, ""),
    ("File: Open...", ID_OPEN, "Ctrl+O"),
    ("File: Open Containing Folder in Explorer", ID_OPEN_FOLDER_EXPLORER, ""),
    ("File: Open Containing Folder in Command Prompt", ID_OPEN_FOLDER_CMD, ""),
    ("File: Open in Default Viewer", ID_OPEN_VIEWER, ""),
    ("File: Open Folder as Workspace...", ID_OPEN_WORKSPACE, ""),
    ("File: Reload from Disk", ID_RELOAD, "Ctrl+R"),
    ("File: Save", ID_SAVE, "Ctrl+S"),
    ("File: Save As...", ID_SAVEAS, "Ctrl+Alt+S"),
    ("File: Save a Copy As...", ID_SAVE_COPY, ""),
    ("File: Save All", ID_SAVEALL, "Ctrl+Shift+S"),
    ("File: Close", ID_CLOSE, "Ctrl+W"),
    ("File: Close All", ID_CLOSE_ALL, "Ctrl+Shift+W"),
    ("File: Close All but Active Document", ID_CLOSE_OTHERS, ""),
    ("File: Close All to the Left", ID_CLOSE_LEFT, ""),
    ("File: Close All to the Right", ID_CLOSE_RIGHT, ""),
    ("File: Close All Unchanged", ID_CLOSE_UNCHANGED, ""),
    ("File: Move to Recycle Bin", ID_RECYCLE, ""),
    ("File: Load Session...", ID_LOAD_SESSION, ""),
    ("File: Save Session...", ID_SAVE_SESSION, ""),
    ("File: Print...", ID_PRINT, ""),
    ("File: Print Now", ID_PRINT_NOW, ""),
    ("File: Page Setup...", ID_PAGE_SETUP, ""),
    ("File: Encrypt This File", ID_ENCRYPT, ""),
    ("File: Remove Encryption From This File", ID_DECRYPT, ""),
    ("Settings: Encryption...", ID_ENC_SETTINGS, ""),
    ("Tools: Passwords (Bitwarden)...", ID_PASSWORDS, ""),
    ("Tools: Lock Passwords (Bitwarden)", ID_PW_LOCK, ""),
    ("View: Log Follow (.log / .out / .err)", ID_LOG_FOLLOW, ""),
    ("Tab: Pin / Unpin Current Tab", ID_TAB_PIN, ""),
    ("Tab: Clear Colour Mark", ID_TAB_COLOR_NONE, ""),
    ("Tab: Compare with File...", ID_TAB_CMP_FILE, ""),
    ("Tab: Compare with Clipboard", ID_TAB_CMP_CLIP, ""),
    ("Tab: Reveal in File Tree", ID_TAB_REVEAL, ""),
    ("Settings: Lock Encryption", ID_ENC_LOCK, ""),
    ("Tools: Send HTTP Request (Ctrl+Enter in a .http file)", ID_HTTP_SEND, ""),
    ("File: Restore Recent Closed File", ID_RESTORE_CLOSED, "Ctrl+Shift+T"),
    ("File: Open All Recent Files", ID_OPEN_ALL_RECENT, ""),
    ("File: Empty Recent Files List", ID_RECENT_CLEAR, ""),
    ("View: Toggle Folder Workspace", ID_WORKSPACE_TOGGLE, ""),
    ("File: Quick Open...", ID_QUICKOPEN, "Ctrl+P"),
    ("Edit: Undo", ID_UNDO, "Ctrl+Z"),
    ("Edit: Cut", ID_CUT, "Ctrl+X"),
    ("Edit: Copy", ID_COPY, "Ctrl+C"),
    ("Edit: Paste", ID_PASTE, "Ctrl+V"),
    ("Settings: Preferences...", ID_PREFS, ""),
    ("Settings: Style Configurator...", ID_STYLE_CFG, ""),
    ("Settings: Shortcut Mapper...", ID_SHORTCUTS, ""),
    ("Settings: Import Style Theme...", ID_IMPORT_THEME, ""),
    ("Settings: Export Style Theme...", ID_EXPORT_THEME, ""),
    ("Settings: Edit Popup ContextMenu", ID_EDIT_CONTEXT, ""),
    ("Tools: Base64 Encode", ID_B64_ENC, ""),
    ("Tools: Base64 Decode", ID_B64_DEC, ""),
    ("Tools: URL Encode", ID_URL_ENC, ""),
    ("Tools: URL Decode", ID_URL_DEC, ""),
    ("Tools: JSON Pretty-print", ID_JSON_PRETTY, ""),
    ("Tools: JSON Minify", ID_JSON_MIN, ""),
    ("Tools: XML Pretty-print", ID_XML_PRETTY, ""),
    ("Tools: XML Minify", ID_XML_MIN, ""),
    ("Tools: MD5 Generate...", ID_HASH_BASE, ""),
    ("Tools: MD5 from Selection into Clipboard", ID_HASH_BASE + 2, ""),
    ("Tools: SHA-1 Generate...", ID_HASH_BASE + 10, ""),
    ("Tools: SHA-1 from Selection into Clipboard", ID_HASH_BASE + 12, ""),
    ("Tools: SHA-256 Generate...", ID_HASH_BASE + 20, ""),
    ("Tools: SHA-256 from Selection into Clipboard", ID_HASH_BASE + 22, ""),
    ("Tools: SHA-512 Generate...", ID_HASH_BASE + 30, ""),
    ("Tools: SHA-512 from Selection into Clipboard", ID_HASH_BASE + 32, ""),
    ("Window: Windows...", ID_WINDOWS, ""),
    ("Edit: Redo", ID_REDO, "Ctrl+Y"),
    ("Edit: Select All", ID_SELALL, "Ctrl+A"),
    ("Edit: Duplicate Line", ID_L_DUP, "Ctrl+D"),
    ("Edit: Delete Line", ID_L_DEL, "Ctrl+Shift+L"),
    ("Edit: Move Line Up", ID_L_UP, "Ctrl+Shift+Up"),
    ("Edit: Move Line Down", ID_L_DOWN, "Ctrl+Shift+Down"),
    ("Edit: Sort Lines Ascending", ID_L_SORT_ASC, ""),
    ("Edit: Sort Lines Descending", ID_L_SORT_DESC, ""),
    ("Edit: Remove Duplicate Lines", ID_L_DEDUP, ""),
    ("Edit: Remove Empty Lines", ID_L_NOEMPTY, ""),
    ("Edit: Trim Trailing Space", ID_L_TRIM, ""),
    ("Edit: UPPERCASE", ID_C_UPPER, "Ctrl+Shift+U"),
    ("Edit: lowercase", ID_C_LOWER, "Ctrl+U"),
    ("Edit: Toggle Line Comment", ID_COMMENT, "Ctrl+Q"),
    ("Edit: Single Line Comment", ID_CMT_LINE, "Ctrl+K"),
    ("Edit: Single Line Uncomment", ID_UNCMT_LINE, "Ctrl+Shift+K"),
    ("Edit: Block Comment", ID_CMT_BLOCK, "Ctrl+Shift+Q"),
    ("Edit: Block Uncomment", ID_UNCMT_BLOCK, "Ctrl+Shift+R"),
    ("Edit: Delete", ID_DELETE, "Del"),
    ("Edit: Begin/End Select", ID_BEGIN_END_SEL, "Ctrl+Shift+B"),
    ("Edit: Begin/End Select in Column Mode", ID_BEGIN_END_COL, "Alt+Shift+B"),
    ("Edit: Insert Date Time (short)", ID_INS_DATE_SHORT, ""),
    ("Edit: Insert Date Time (long)", ID_INS_DATE_LONG, ""),
    ("Edit: Insert Date Time (customized)", ID_INS_DATE_CUSTOM, ""),
    ("Edit: Copy Current Full File Path", ID_CP_FULLPATH, ""),
    ("Edit: Copy Current Filename", ID_CP_NAME, ""),
    ("Edit: Copy Current Dir. Path", ID_CP_DIR, ""),
    ("Edit: Copy All Filenames", ID_CP_ALLNAMES, ""),
    ("Edit: Copy All File Paths", ID_CP_ALLPATHS, ""),
    ("Edit: Increase Line Indent", ID_INDENT_INC, "Tab"),
    ("Edit: Decrease Line Indent", ID_INDENT_DEC, "Shift+Tab"),
    ("Edit: Proper Case", ID_CASE_PROPER, "Alt+U"),
    ("Edit: Proper Case (blend)", ID_CASE_PROPER_BLEND, "Alt+Shift+U"),
    ("Edit: Sentence case", ID_CASE_SENTENCE, "Ctrl+Alt+U"),
    ("Edit: Sentence case (blend)", ID_CASE_SENTENCE_BLEND, "Ctrl+Alt+Shift+U"),
    ("Edit: iNVERT cASE", ID_CASE_INVERT, ""),
    ("Edit: ranDOm CasE", ID_CASE_RANDOM, ""),
    ("Edit: Remove Consecutive Duplicate Lines", ID_L_RM_CONSEC, ""),
    ("Edit: Remove Empty Lines (Containing Blank characters)", ID_L_RM_BLANK, ""),
    ("Edit: Split Lines", ID_L_SPLIT, "Ctrl+I"),
    ("Edit: Join Lines", ID_L_JOIN, "Ctrl+J"),
    ("Edit: Transpose Current Line", ID_L_TRANSPOSE, "Ctrl+T"),
    ("Edit: Insert Blank Line Above Current", ID_L_INS_ABOVE, "Ctrl+Alt+Enter"),
    ("Edit: Insert Blank Line Below Current", ID_L_INS_BELOW, "Ctrl+Alt+Shift+Enter"),
    ("Edit: Reverse Line Order", ID_L_REVERSE, ""),
    ("Edit: Randomize Line Order", ID_L_RANDOM, ""),
    ("Edit: Cut Current Line", ID_L_CUT, "Ctrl+L"),
    ("Edit: Copy Current Line", ID_L_COPY, "Ctrl+Shift+X"),
    ("Edit: Sort Lines Ignoring Case Ascending", ID_L_SORT_NOCASE_ASC, ""),
    ("Edit: Sort Lines Ignoring Case Descending", ID_L_SORT_NOCASE_DESC, ""),
    ("Edit: Sort Lines As Integers Ascending", ID_L_SORT_INT_ASC, ""),
    ("Edit: Sort Lines As Integers Descending", ID_L_SORT_INT_DESC, ""),
    ("Edit: Sort Lines As Decimals (Dot) Ascending", ID_L_SORT_DOT_ASC, ""),
    ("Edit: Sort Lines As Decimals (Dot) Descending", ID_L_SORT_DOT_DESC, ""),
    ("Edit: Sort Lines As Decimals (Comma) Ascending", ID_L_SORT_COMMA_ASC, ""),
    ("Edit: Sort Lines As Decimals (Comma) Descending", ID_L_SORT_COMMA_DESC, ""),
    ("Edit: Function Completion", ID_AC_FUNC, "Ctrl+Space"),
    ("Edit: Word Completion", ID_AC_WORD, "Ctrl+Enter"),
    ("Edit: Function Parameters Hint", ID_AC_HINT, "Ctrl+Shift+Space"),
    ("Edit: Path Completion", ID_AC_PATH, "Ctrl+Alt+Space"),
    ("Edit: Trim Leading Space", ID_B_TRIM_LEAD, ""),
    ("Edit: Trim Leading and Trailing Space", ID_B_TRIM_BOTH, ""),
    ("Edit: EOL to Space", ID_B_EOL_SPACE, ""),
    ("Edit: Remove Unnecessary Blank and EOL", ID_B_UNNECESSARY, ""),
    ("Edit: TAB to Space", ID_B_TAB_SPACE, ""),
    ("Edit: Space to TAB (All)", ID_B_SPACE_TAB_ALL, ""),
    ("Edit: Space to TAB (Leading)", ID_B_SPACE_TAB_LEAD, ""),
    ("Edit: Paste HTML Content", ID_PS_HTML, ""),
    ("Edit: Paste RTF Content", ID_PS_RTF, ""),
    ("Edit: Copy Binary Content", ID_PS_COPY_BIN, ""),
    ("Edit: Cut Binary Content", ID_PS_CUT_BIN, ""),
    ("Edit: Paste Binary Content", ID_PS_PASTE_BIN, ""),
    ("Edit: Open Selected File", ID_SEL_OPEN, ""),
    ("Edit: Open Selected Path in Explorer", ID_SEL_FOLDER, ""),
    ("Edit: Search Selection on Internet", ID_SEL_SEARCH, ""),
    ("Edit: Column Editor...", ID_COLUMN_EDITOR, "Alt+C"),
    ("Edit: Character Panel", ID_CHAR_PANEL, ""),
    ("Edit: Clipboard History", ID_CLIP_HISTORY, ""),
    ("Edit: Set Read-Only", ID_READONLY, ""),
    ("Edit: Clear Read-Only Flag", ID_CLEAR_RO, ""),
    ("Edit: EOL to Windows (CR LF)", ID_EOL_CRLF, ""),
    ("Edit: EOL to Unix (LF)", ID_EOL_LF, ""),
    ("Edit: EOL to Macintosh (CR)", ID_EOL_CR, ""),
    ("Search: Find...", ID_FIND, "Ctrl+F"),
    ("Search: Replace...", ID_REPLACE, "Ctrl+H"),
    ("Search: Find in Files...", ID_FIF, "Ctrl+Shift+F"),
    ("Search: Mark...", ID_MARK, "Ctrl+M"),
    ("Search: Find Next", ID_FINDNEXT, "F3"),
    ("Search: Select and Find Next", ID_SEL_FIND_NEXT, "Ctrl+F3"),
    ("Search: Select and Find Previous", ID_SEL_FIND_PREV, "Ctrl+Shift+F3"),
    ("Search: Find (Volatile) Next", ID_VOL_NEXT, "Ctrl+Alt+F3"),
    ("Search: Find (Volatile) Previous", ID_VOL_PREV, "Ctrl+Alt+Shift+F3"),
    ("Search: Incremental Search", ID_INCR, "Ctrl+Alt+I"),
    ("Search: Search Results Window", ID_RESULTS, "F7"),
    ("Search: Next Search Result", ID_NEXT_RESULT, "F4"),
    ("Search: Previous Search Result", ID_PREV_RESULT, "Shift+F4"),
    ("Search: Go to Matching Brace", ID_BRACE_GOTO, "Ctrl+B"),
    ("Search: Select All Between Matching Braces", ID_BRACE_SEL, "Ctrl+Alt+B"),
    ("Search: Style All Occurrences of Token (1st)", ID_STYLE_ALL_BASE, ""),
    ("Search: Clear all Styles", ID_CLEAR_STYLE_BASE + 5, ""),
    ("Search: Find characters in range...", ID_FIND_CHARS, ""),
    ("Bookmark: Cut Bookmarked Lines", ID_BM_CUT, ""),
    ("Bookmark: Copy Bookmarked Lines", ID_BM_COPY, ""),
    ("Bookmark: Paste to (Replace) Bookmarked Lines", ID_BM_PASTE, ""),
    ("Bookmark: Remove Bookmarked Lines", ID_BM_DELETE, ""),
    ("Bookmark: Remove Non-Bookmarked Lines", ID_BM_DELETE_UNMARKED, ""),
    ("Bookmark: Inverse", ID_BM_INVERT, ""),
    ("Search: Find Previous", ID_FINDPREV, "Shift+F3"),
    ("Search: Go to Line...", ID_GOTO, "Ctrl+G"),
    ("Bookmark: Toggle", ID_BM_TOGGLE, "Ctrl+F2"),
    ("Bookmark: Next", ID_BM_NEXT, "F2"),
    ("Bookmark: Previous", ID_BM_PREV, "Shift+F2"),
    ("Bookmark: Clear All", ID_BM_CLEAR, ""),
    ("View: Toggle Word Wrap", ID_WRAP, ""),
    ("View: Always on Top", ID_ON_TOP, ""),
    ("View: Toggle Full Screen Mode", ID_FULLSCREEN, "F11"),
    ("View: Post-It", ID_POSTIT, "F12"),
    ("View: Distraction Free Mode", ID_DISTRACTION, ""),
    ("View: Show Space and Tab", ID_SYM_WS, ""),
    ("View: Show End of Line", ID_SYM_EOL, ""),
    ("View: Show All Characters", ID_SYM_ALL, ""),
    ("View: Show Indent Guide", ID_SYM_INDENT, ""),
    ("View: Show Wrap Symbol", ID_SYM_WRAP, ""),
    ("View: Hide Lines", ID_HIDE_LINES, "Alt+H"),
    ("View: Fold All", ID_FOLD_ALL, "Alt+0"),
    ("View: Unfold All", ID_UNFOLD_ALL, "Alt+Shift+0"),
    ("View: Fold Current Level", ID_FOLD_CUR, "Ctrl+Alt+F"),
    ("View: Unfold Current Level", ID_UNFOLD_CUR, "Ctrl+Alt+Shift+F"),
    ("View: Summary...", ID_SUMMARY, ""),
    ("View: Document List", ID_DOC_LIST, ""),
    ("View: Function List", ID_FUNC_LIST, ""),
    ("View: Text Direction RTL", ID_RTL, "Ctrl+Alt+R"),
    ("View: Text Direction LTR", ID_LTR, "Ctrl+Alt+L"),
    ("View: Monitoring (tail -f)", ID_MONITOR, ""),
    ("View: Move Tab Forward", ID_MOVE_TAB_FWD, "Ctrl+Shift+PgDn"),
    ("View: Move Tab Backward", ID_MOVE_TAB_BACK, "Ctrl+Shift+PgUp"),
    ("View: Open in New Instance", ID_OPEN_NEW_INSTANCE, ""),
    ("View: Move to New Instance", ID_MOVE_NEW_INSTANCE, ""),
    ("View: Toggle Live Preview", ID_PREVIEW, "Ctrl+Shift+V"),
    ("View: Markdown Toolbar", ID_MD_TOOLBAR, ""),
    ("Markdown: Bold", ID_MD_BOLD, ""),
    ("Markdown: Italic", ID_MD_ITALIC, ""),
    ("Markdown: Strikethrough", ID_MD_STRIKE, ""),
    ("Markdown: Inline Code", ID_MD_CODE, ""),
    ("Markdown: Heading 1", ID_MD_H1, ""),
    ("Markdown: Heading 2", ID_MD_H2, ""),
    ("Markdown: Heading 3", ID_MD_H3, ""),
    ("Markdown: Heading 4", ID_MD_H4, ""),
    ("Markdown: Heading 5", ID_MD_H5, ""),
    ("Markdown: Heading 6", ID_MD_H6, ""),
    ("Markdown: Quote", ID_MD_QUOTE, ""),
    ("Markdown: Bulleted List", ID_MD_BULLET, ""),
    ("Markdown: Numbered List", ID_MD_NUMBERED, ""),
    ("Markdown: Task List", ID_MD_TASK, ""),
    ("Markdown: Link", ID_MD_LINK, ""),
    ("Markdown: Image", ID_MD_IMAGE, ""),
    ("Markdown: Table", ID_MD_TABLE, ""),
    ("Markdown: Code Block", ID_MD_CODEBLOCK, ""),
    ("Markdown: Horizontal Rule", ID_MD_RULE, ""),
    ("View: Toggle Dark Mode", ID_THEME, ""),
    ("View: Toggle Minimap", ID_MINIMAP, ""),
    ("View: Zoom In", ID_ZOOMIN, "Ctrl+Num +"),
    ("View: Zoom Out", ID_ZOOMOUT, "Ctrl+Num -"),
    ("View: Reset Zoom", ID_ZOOMRESET, ""),
    ("View: Next Tab", ID_NEXT_TAB, "Ctrl+Tab"),
    ("View: Previous Tab", ID_PREV_TAB, "Ctrl+Shift+Tab"),
    ("Help: About Note++", ID_ABOUT, ""),
    ("Help: Check for Updates Now", ID_UPDATE_CHECK, ""),
    ("Help: Check for Updates Automatically", ID_UPDATE_AUTO, ""),
];
