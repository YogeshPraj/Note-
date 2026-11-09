//! How a docked tab and the main editor share the space: which way the divider runs, which pane comes first, and where the
//! divider goes. Plain numbers, so any toolkit can draw it (see `docs/winui3-plan.md`, phase 1).

use crate::session::DockSide;

/// The split for a dock on one side, for a space `room` pixels long along the divider's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockLayout {
    /// The divider runs top to bottom: the panes are side by side.
    pub side_by_side: bool,
    /// The docked pane is the first one: left of or above the main editor.
    pub dock_first: bool,
    /// Where the divider goes, from the start of the space.
    pub divider: i32,
}

impl DockSide {
    /// The panes sit side by side (left and right), rather than one above the other.
    pub fn side_by_side(self) -> bool {
        matches!(self, DockSide::Left | DockSide::Right)
    }

    /// The docked pane comes before the main one.
    pub fn dock_first(self) -> bool {
        matches!(self, DockSide::Left | DockSide::Top)
    }
}

/// The split for a dock on `side` taking `share` thousandths of `room`.
pub fn layout(side: DockSide, share: u32, room: i32) -> DockLayout {
    let dock = room.max(0) * crate::num::to_i32(share.min(1000)) / 1000;
    let divider = if side.dock_first() { dock } else { room.max(0) - dock };
    DockLayout { side_by_side: side.side_by_side(), dock_first: side.dock_first(), divider }
}

/// The share of `room` the docked pane has with the divider at `divider`, in thousandths; None when the space has no size yet
/// (a window being laid out), which must not overwrite the share the user left.
pub fn share_from(side: DockSide, divider: i32, room: i32) -> Option<u32> {
    if room <= 0 {
        return None;
    }
    let dock = if side.dock_first() { divider } else { room - divider };
    Some(crate::num::to_u32(dock.clamp(0, room) * 1000 / room))
}

/// What a pane's header says: whose tab, whether it has unsaved changes, and whether the user is in that pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneHeading {
    pub title: String,
    pub modified: bool,
    pub current: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn left_and_right_sit_side_by_side_and_left_and_top_come_first() {
        assert!(DockSide::Left.side_by_side() && DockSide::Right.side_by_side());
        assert!(!DockSide::Top.side_by_side() && !DockSide::Bottom.side_by_side());
        assert!(DockSide::Left.dock_first() && DockSide::Top.dock_first());
        assert!(!DockSide::Right.dock_first() && !DockSide::Bottom.dock_first());
    }

    #[test]
    fn the_divider_leaves_the_dock_its_share_on_either_side() {
        assert_eq!(layout(DockSide::Left, 300, 1000), DockLayout { side_by_side: true, dock_first: true, divider: 300 });
        assert_eq!(layout(DockSide::Right, 300, 1000), DockLayout { side_by_side: true, dock_first: false, divider: 700 });
        assert_eq!(layout(DockSide::Top, 250, 800).divider, 200);
        assert_eq!(layout(DockSide::Bottom, 250, 800).divider, 600);
    }

    #[test]
    fn a_share_comes_back_from_where_the_divider_was_left() {
        for side in DockSide::ALL {
            let at = layout(side, 420, 1000).divider;
            assert_eq!(share_from(side, at, 1000), Some(420), "{side:?}");
        }
    }

    #[test]
    fn a_space_with_no_size_neither_gives_nor_takes_a_share() {
        assert_eq!(share_from(DockSide::Left, 10, 0), None);
        assert_eq!(layout(DockSide::Right, 500, -5).divider, 0);
    }

    #[test]
    fn a_divider_dragged_past_the_edge_stays_a_sensible_share() {
        assert_eq!(share_from(DockSide::Left, 1500, 1000), Some(1000));
        assert_eq!(share_from(DockSide::Right, 1500, 1000), Some(0));
        assert_eq!(layout(DockSide::Left, 5000, 1000).divider, 1000);
    }
}
