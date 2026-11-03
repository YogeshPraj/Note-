//! Style colours of each lexer, from the theme.

use super::*;

/// Style colours of a lexer: (style id, colour). Unknown lexers have none.
pub fn palette(lexer: &str, t: &Theme) -> Vec<(usize, Rgb)> {
    [palette_1, palette_2, palette_3, palette_4].iter().map(|f| f(lexer, t)).find(|v| !v.is_empty()).unwrap_or_default()
}

pub(super) fn palette_1(lexer: &str, t: &Theme) -> Vec<(usize, Rgb)> {
    let (c, n, k, s, ty, m, f, p) = (t.comment, t.number, t.keyword, t.string, t.ty, t.mac, t.func, t.prop);
    match lexer {
        // SCE_RUST_*
        "rust" => {
            vec![(1, c), (2, c), (3, c), (4, c), (5, n), (6, k), (7, ty), (8, ty), (13, s), (14, s), (15, s), (18, ty), (19, m), (21, s), (22, s), (23, s)]
        }
        // SCE_C_*
        "cpp" => vec![(1, c), (2, c), (3, c), (15, c), (4, n), (5, k), (16, ty), (6, s), (7, s), (13, s), (14, s), (9, m)],
        // SCE_P_*
        "python" => vec![(1, c), (12, c), (2, n), (3, s), (4, s), (6, s), (7, s), (16, s), (5, k), (14, ty), (8, ty), (9, f), (15, m)],
        // SCE_JSON_*
        "json" => vec![(1, n), (2, s), (4, p), (6, c), (7, c), (11, k), (12, k)],
        // SCE_YAML_*
        "yaml" => vec![(1, c), (3, k), (4, n), (5, m), (6, m)],
        // SCE_MARKDOWN_*
        "markdown" => vec![(6, k), (7, k), (8, k), (9, k), (10, k), (11, k), (13, m), (14, m), (15, c), (17, c), (18, p), (19, s), (20, s), (21, s)],
        // SCE_H_* (HTML and XML share these)
        "hypertext" | "xml" => vec![(1, k), (2, k), (11, k), (3, p), (4, p), (5, n), (6, s), (7, s), (9, c), (10, m)],
        _ => Vec::new(),
    }
}

pub(super) fn palette_4(lexer: &str, t: &Theme) -> Vec<(usize, Rgb)> {
    let (c, n, k, s, ty, m, f, p) = (t.comment, t.number, t.keyword, t.string, t.ty, t.mac, t.func, t.prop);
    match lexer {
        // SCE_CSS_*
        "css" => vec![(1, k), (2, ty), (3, m), (4, m), (6, p), (7, p), (8, s), (9, c), (10, m), (11, k), (12, k), (13, s), (14, s), (15, p), (16, p)],
        // SCE_SQL_*
        "sql" | "mysql" | "mssql" => vec![(1, c), (2, c), (3, c), (15, c), (17, c), (4, n), (5, k), (16, ty), (19, ty), (20, ty), (6, s), (7, s), (23, p)],
        // SCE_SH_*
        "bash" => vec![(2, c), (3, n), (4, k), (5, s), (6, s), (9, m), (10, m), (11, s), (12, m), (13, s)],
        // SCE_LUA_*
        "lua" => vec![(1, c), (2, c), (3, c), (4, n), (5, k), (6, s), (7, s), (8, s), (9, m), (13, ty), (14, ty), (20, m)],
        // SCE_RB_*
        "ruby" => vec![
            (2, c),
            (3, c),
            (19, c),
            (4, n),
            (5, k),
            (6, s),
            (7, s),
            (8, ty),
            (9, f),
            (12, s),
            (13, m),
            (14, p),
            (15, ty),
            (16, m),
            (17, m),
            (18, s),
            (24, s),
            (25, s),
            (26, s),
            (27, s),
            (28, s),
        ],
        // SCE_PL_*
        "perl" => vec![
            (2, c),
            (3, c),
            (31, c),
            (4, n),
            (5, k),
            (6, s),
            (7, s),
            (9, m),
            (12, m),
            (13, m),
            (14, m),
            (17, s),
            (18, s),
            (19, s),
            (20, s),
            (26, s),
            (27, s),
            (28, s),
            (29, s),
            (30, s),
        ],
        // SCE_POWERSHELL_*
        "powershell" => vec![(1, c), (13, c), (2, s), (3, s), (14, s), (15, s), (4, n), (5, m), (8, k), (9, f), (10, ty), (11, f)],
        _ => Vec::new(),
    }
}

pub(super) fn palette_2(lexer: &str, t: &Theme) -> Vec<(usize, Rgb)> {
    let (c, n, k, s, ty, m, f, p) = (t.comment, t.number, t.keyword, t.string, t.ty, t.mac, t.func, t.prop);
    match lexer {
        // SCE_BAT_*
        "batch" => vec![(1, c), (2, k), (3, m), (5, ty)],
        // SCE_PROPS_*
        "props" => vec![(1, c), (2, k), (4, s), (5, f)],
        // SCE_TOML_*
        "toml" => vec![(1, c), (3, k), (4, n), (5, k), (6, p), (9, s), (10, s), (11, s), (12, s), (13, m), (14, n)],
        // SCE_DIFF_*
        "diff" => vec![
            (1, c),
            (2, k),
            (3, k),
            (4, m),
            (5, (0xCC, 0x33, 0x33)),
            (6, (0x22, 0x99, 0x22)),
            (7, (0xCC, 0x88, 0x00)),
            (8, (0x22, 0x99, 0x22)),
            (9, (0xCC, 0x33, 0x33)),
        ],
        // SCE_MAKE_*
        "makefile" => vec![(1, c), (2, m), (3, p), (5, f)],
        // SCE_B_* (Visual Basic and relatives)
        "vb" | "vbscript" | "freebasic" | "blitzbasic" | "purebasic" | "powerbasic" => {
            vec![(1, c), (19, c), (20, c), (21, c), (2, n), (17, n), (18, n), (3, k), (10, k), (11, k), (12, k), (4, s), (5, m), (13, m)]
        }
        // SCE_PAS_*
        "pascal" => vec![(2, c), (3, c), (4, c), (5, m), (6, m), (7, n), (8, n), (9, k), (10, s), (12, s), (15, s)],
        // SCE_L_*
        "latex" => vec![(1, k), (2, ty), (3, s), (4, c), (5, ty), (6, s), (7, c), (8, s), (11, p)],
        // SCE_ASM_*
        "asm" => vec![(1, c), (11, c), (15, c), (2, n), (3, s), (12, s), (6, k), (7, k), (14, k), (8, ty), (9, m)],
        // SCE_F_*
        "fortran" | "f77" => vec![(1, c), (2, n), (3, s), (4, s), (8, k), (9, ty), (10, ty), (11, m)],
        // SCE_LISP_*
        "lisp" => vec![(1, c), (12, c), (2, n), (3, k), (4, k), (5, ty), (6, s), (11, m)],
        // SCE_HA_*
        "haskell" | "literatehaskell" => {
            vec![(2, k), (3, n), (4, s), (5, s), (6, ty), (7, ty), (8, ty), (9, ty), (10, k), (13, c), (14, c), (15, c), (16, c), (17, m), (18, m)]
        }
        // SCE_TCL_*
        "tcl" => vec![(1, c), (2, c), (20, c), (21, c), (3, n), (4, k), (5, s), (8, m), (9, m), (12, k), (13, k), (14, k), (15, k)],
        // SCE_R_*
        "r" => vec![(1, c), (2, k), (3, k), (4, k), (5, n), (6, s), (7, s), (10, m)],
        _ => Vec::new(),
    }
}

pub(super) fn palette_3(lexer: &str, t: &Theme) -> Vec<(usize, Rgb)> {
    let (c, n, k, s, ty, m, f, p) = (t.comment, t.number, t.keyword, t.string, t.ty, t.mac, t.func, t.prop);
    match lexer {
        // SCE_NSIS_*
        "nsis" => vec![(1, c), (18, c), (2, s), (3, s), (4, s), (5, k), (6, m), (7, ty), (9, k), (14, n), (17, k)],
        // SCE_INNO_*
        "inno" => vec![(1, c), (7, c), (2, k), (8, k), (3, p), (4, ty), (5, m), (10, s), (11, s)],
        // SCE_MATLAB_*
        "matlab" | "octave" => vec![(1, c), (2, k), (3, n), (4, k), (5, s), (8, s)],
        // SCE_ERLANG_*
        "erlang" => vec![(1, c), (14, c), (15, c), (16, c), (2, m), (3, n), (4, k), (5, s), (7, ty), (8, f), (9, s), (10, m), (11, m)],
        // SCE_CMAKE_*
        "cmake" => vec![(1, c), (2, s), (3, s), (4, s), (5, k), (6, ty), (7, m), (8, f), (9, k), (10, k), (11, k), (12, k), (14, n)],
        // SCE_ADA_*
        "ada" => vec![(1, k), (3, n), (5, s), (7, s), (10, c), (9, m)],
        // SCE_D_*
        "d" => vec![(1, c), (2, c), (3, c), (4, c), (15, c), (5, n), (6, k), (7, ty), (8, ty), (9, ty), (10, s), (12, s), (18, s), (19, s)],
        // SCE_V_*
        "verilog" => vec![(1, c), (2, c), (3, c), (4, n), (5, k), (6, s), (7, ty), (8, ty), (9, m)],
        // SCE_VHDL_*
        "vhdl" => vec![(1, c), (2, c), (15, c), (3, n), (4, s), (8, k), (9, ty), (10, ty), (11, f), (12, ty), (13, ty)],
        // SCE_HPHP_* (PHP lives in the high style numbers of the HTML lexer)
        "phpscript" => vec![(124, c), (125, c), (119, s), (120, s), (126, s), (122, n), (121, k), (123, m)],
        // SCE_CAML_*
        "caml" => vec![(3, k), (4, ty), (5, ty), (8, n), (9, s), (11, s), (12, c), (13, c), (14, c), (15, c)],
        // SCE_NIM_*
        "nim" | "nimrod" => vec![(1, c), (2, c), (3, c), (4, c), (5, n), (6, s), (7, s), (8, k), (9, s), (10, s), (12, f)],
        // SCE_JULIA_*
        "julia" => vec![(1, c), (2, n), (3, k), (4, k), (5, k), (20, k), (6, s), (10, s), (12, m), (14, s), (15, s), (18, ty)],
        // SCE_COFFEESCRIPT_* mirrors the C family
        "coffeescript" => vec![(1, c), (2, c), (3, c), (22, c), (4, n), (5, k), (6, s), (7, s), (14, s), (9, m)],
        _ => Vec::new(),
    }
}
