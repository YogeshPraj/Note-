//! Line, case, comment, blank and completion commands.

use super::*;

pub(super) fn command_core_3<S: sci::Send + Copy>(ws: &mut Workspace<S>, flash: &mut Option<String>, id: usize) -> bool {
    let sci = ws.sci;
    match id {
        ID_EOL_CR => *flash = ws.set_eol(Eol::Cr),
        ID_WRAP => {
            let on = !ws.wrap;
            ws.set_wrap(on);
        }
        ID_THEME => {
            let next = if ws.theme.dark { Theme::light() } else { Theme::dark() };
            ws.set_theme(next);
        }
        _ if md_fmt(id).is_some() => {
            if let Some(f) = md_fmt(id) {
                *flash = ws.md_format(f);
            }
        }
        ID_ZOOMIN => drop(sci.send(sci::SCI_ZOOMIN, 0, 0)),
        ID_ZOOMOUT => drop(sci.send(sci::SCI_ZOOMOUT, 0, 0)),
        ID_ZOOMRESET => drop(sci.send(sci::SCI_SETZOOM, 0, 0)),
        // With a selection this repeats the selected text; with none, the whole line.
        ID_L_DUP => drop(sci.send(sci::SCI_SELECTIONDUPLICATE, 0, 0)),
        ID_L_DEL => drop(sci.send(sci::SCI_LINEDELETE, 0, 0)),
        ID_L_UP => drop(sci.send(sci::SCI_MOVESELECTEDLINESUP, 0, 0)),
        ID_L_DOWN => drop(sci.send(sci::SCI_MOVESELECTEDLINESDOWN, 0, 0)),
        ID_C_UPPER => drop(sci.send(sci::SCI_UPPERCASE, 0, 0)),
        ID_C_LOWER => drop(sci.send(sci::SCI_LOWERCASE, 0, 0)),
        ID_L_SORT_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::Lexicographic, false)),
        ID_L_SORT_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::Lexicographic, true)),
        ID_L_SORT_NOCASE_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::IgnoreCase, false)),
        ID_L_SORT_NOCASE_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::IgnoreCase, true)),
        ID_L_SORT_INT_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::Integer, false)),
        ID_L_SORT_INT_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::Integer, true)),
        ID_L_SORT_DOT_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalDot, false)),
        ID_L_SORT_DOT_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalDot, true)),
        ID_L_SORT_COMMA_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalComma, false)),
        ID_L_SORT_COMMA_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalComma, true)),
        ID_L_DEDUP => *flash = ws.line_op(LineOp::RemoveDuplicates),
        ID_L_RM_CONSEC => *flash = ws.line_op(LineOp::RemoveConsecutiveDuplicates),
        ID_L_NOEMPTY => *flash = ws.line_op(LineOp::RemoveEmpty),
        ID_L_RM_BLANK => *flash = ws.line_op(LineOp::RemoveBlank),
        ID_L_REVERSE => *flash = ws.line_op(LineOp::Reverse),
        ID_L_RANDOM => *flash = ws.line_op(LineOp::Randomize),
        ID_L_JOIN => *flash = ws.line_op(LineOp::Join),
        ID_L_SPLIT => *flash = ws.line_op(LineOp::Split),
        ID_L_TRIM => *flash = ws.line_op(LineOp::TrimTrailing),
        ID_B_TRIM_LEAD => *flash = ws.line_op(LineOp::TrimLeading),
        ID_B_TRIM_BOTH => *flash = ws.line_op(LineOp::TrimBoth),
        _ => return false,
    }
    true
}

pub(super) fn command_core_4<S: sci::Send + Copy>(ws: &mut Workspace<S>, flash: &mut Option<String>, id: usize) -> bool {
    let sci = ws.sci;
    match id {
        ID_B_EOL_SPACE => *flash = ws.line_op(LineOp::EolToSpace),
        ID_B_UNNECESSARY => *flash = ws.line_op(LineOp::RemoveUnnecessaryBlanks),
        ID_B_TAB_SPACE => *flash = ws.line_op(LineOp::TabsToSpaces),
        ID_B_SPACE_TAB_ALL => *flash = ws.line_op(LineOp::SpacesToTabsAll),
        ID_B_SPACE_TAB_LEAD => *flash = ws.line_op(LineOp::SpacesToTabsLeading),
        ID_COMMENT => *flash = ws.line_op(LineOp::ToggleComment),
        ID_CMT_LINE => *flash = ws.line_op(LineOp::Comment),
        ID_UNCMT_LINE => *flash = ws.line_op(LineOp::Uncomment),
        ID_CMT_BLOCK => *flash = ws.line_op(LineOp::BlockComment),
        ID_UNCMT_BLOCK => *flash = ws.line_op(LineOp::BlockUncomment),
        ID_L_INS_ABOVE | ID_L_INS_BELOW => ws.insert_blank_line(id == ID_L_INS_ABOVE),
        ID_L_CUT => drop(sci.send(sci::SCI_LINECUT, 0, 0)),
        ID_L_COPY => drop(sci.send(sci::SCI_LINECOPY, 0, 0)),
        ID_L_TRANSPOSE => drop(sci.send(sci::SCI_LINETRANSPOSE, 0, 0)),
        ID_CASE_PROPER => *flash = ws.case_op(CaseOp::Proper),
        ID_CASE_PROPER_BLEND => *flash = ws.case_op(CaseOp::ProperBlend),
        ID_CASE_SENTENCE => *flash = ws.case_op(CaseOp::Sentence),
        ID_CASE_SENTENCE_BLEND => *flash = ws.case_op(CaseOp::SentenceBlend),
        ID_CASE_INVERT => *flash = ws.case_op(CaseOp::Invert),
        ID_CASE_RANDOM => *flash = ws.case_op(CaseOp::Random),
        ID_DELETE => drop(sci.send(sci::SCI_CLEAR, 0, 0)),
        ID_BEGIN_END_SEL => *flash = ws.begin_end_select(false),
        ID_BEGIN_END_COL => *flash = ws.begin_end_select(true),
        ID_INDENT_INC | ID_INDENT_DEC => ws.indent(id == ID_INDENT_INC),
        ID_AC_FUNC => *flash = ws.complete(Completion::Functions),
        ID_AC_WORD => *flash = ws.complete(Completion::Words),
        ID_AC_HINT => *flash = ws.complete(Completion::Hint),
        ID_AC_PATH => *flash = ws.complete(Completion::Path),
        _ if (ID_ENGINE_BASE..ID_ENGINE_BASE + edit::SEARCH_ENGINES.len()).contains(&id) => {
            ws.search_engine = id - ID_ENGINE_BASE;
            *flash = Some(format!("Search engine: {}", edit::SEARCH_ENGINES[ws.search_engine].0));
        }
        _ => return false,
    }
    true
}
