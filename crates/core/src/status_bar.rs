//! What the status bar says, in which order, and what clicking each part means. Plain data, so any toolkit can draw it (see
//! `docs/winui3-plan.md`, phase 1); the shell only paints the segments and opens the menu an action names.

use crate::workspace::Status;

/// What clicking a segment does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusAction {
    /// Choose the encoding.
    Encoding,
    /// Choose the line ending.
    LineEnding,
    /// Choose the language.
    Language,
    /// The connected AI agents: their log, and Stop.
    Agents,
}

/// One right-hand part of the bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    /// What a click does; None for a part that only says something.
    pub action: Option<StatusAction>,
}

/// The whole bar: the text at the left, and the right-hand parts, the rightmost first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusBar {
    pub left: String,
    pub segments: Vec<Segment>,
}

fn says(text: &str) -> Segment {
    Segment { text: text.to_string(), action: None }
}

fn does(text: &str, action: StatusAction) -> Segment {
    Segment { text: text.to_string(), action: Some(action) }
}

/// The bar for the active tab. `message` is something just said (it takes the place of the caret's position, once);
/// `overtype` is the Insert key's state; `agents` names the connected AI agents, if any.
pub fn layout(status: &Status, message: Option<String>, overtype: bool, agents: Option<String>) -> StatusBar {
    let mut segments = vec![
        says(if overtype { "OVR" } else { "INS" }),
        does(&status.lang, StatusAction::Language),
        does(&status.eol, StatusAction::LineEnding),
        does(&status.enc, StatusAction::Encoding),
        says(&status.length),
        says(&status.lines),
    ];
    segments.extend(agents.map(|text| Segment { text, action: Some(StatusAction::Agents) }));
    StatusBar { left: message.unwrap_or_else(|| status.position.clone()), segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> Status {
        Status {
            position: "Ln 1, Col 1".into(),
            lines: "lines: 3".into(),
            length: "length: 12".into(),
            eol: "Windows (CR LF)".into(),
            enc: "UTF-8".into(),
            lang: "Normal text".into(),
        }
    }

    #[test]
    fn the_bar_says_where_the_caret_is_and_what_the_document_is() {
        let bar = layout(&status(), None, false, None);
        assert_eq!(bar.left, "Ln 1, Col 1");
        let texts: Vec<&str> = bar.segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["INS", "Normal text", "Windows (CR LF)", "UTF-8", "length: 12", "lines: 3"]);
    }

    #[test]
    fn encoding_line_ending_and_language_can_be_clicked_and_the_counts_cannot() {
        let bar = layout(&status(), None, false, None);
        let action = |text: &str| bar.segments.iter().find(|s| s.text == text).and_then(|s| s.action);
        assert_eq!(action("UTF-8"), Some(StatusAction::Encoding));
        assert_eq!(action("Windows (CR LF)"), Some(StatusAction::LineEnding));
        assert_eq!(action("Normal text"), Some(StatusAction::Language));
        assert_eq!(action("lines: 3"), None);
        assert_eq!(action("INS"), None);
    }

    #[test]
    fn a_message_takes_the_place_of_the_position_and_overtype_shows() {
        let bar = layout(&status(), Some("Saved".into()), true, None);
        assert_eq!(bar.left, "Saved");
        assert_eq!(bar.segments[0].text, "OVR");
    }

    #[test]
    fn connected_agents_come_last_and_open_their_menu() {
        let bar = layout(&status(), None, false, Some("AI connected: claude".into()));
        let last = bar.segments.last().unwrap();
        assert_eq!((last.text.as_str(), last.action), ("AI connected: claude", Some(StatusAction::Agents)));
        assert_eq!(layout(&status(), None, false, None).segments.len(), 6, "no agents, no segment");
    }
}
