# Plan: a WinUI 3 shell for Windows, with macOS and Linux kept

Status: **in progress.** Phase 0 built (not yet proven on the build servers); phase 1's six steps done; phase 2's spike started. Nothing below is approved for implementation yet; each phase needs its own go-ahead, and the
spike (phase 2) decides whether phases 3 and later happen at all.

## Goal

Give Windows users the real Windows 11 experience by building the Windows interface from WinUI 3's own controls, so that on Windows
Note++ maintains no UI components of its own. Keep releasing for macOS and Linux from the same code, with every feature.

## Why

- WinUI 3 already has the components Note++ paints by hand today: menu bar, toolbar, tab strip, settings pages, dialogs,
  notifications, tree, colour picker, list, search box. Themes, DPI, accessibility and keyboard navigation come with them.
- `ui-kit` (about 8,000 lines) exists mostly because native Windows controls cannot be themed. On Windows it would no longer be needed.
- GTK (Linux) and Cocoa (macOS) already follow the system's dark and light themes, so the wx shell could also drop much of `ui-kit`
  and use plain native wx controls once Windows no longer depends on it.
- Rust can drive WinUI 3 through Microsoft's own `windows-reactor` (windows-rs), so the Windows shell can stay in Rust and call `core`
  directly, with no C# and no bridge.

## The shape: one brain, thin faces

Abstraction at the level of **data**, not widgets. `core` decides what is on screen and what every action does, as plain data with
tests; each shell only draws that data with its own toolkit and sends clicks and keys back.

| Layer | Holds | Platforms |
|---|---|---|
| `core` (grows) | behaviour and rules: commands, menus, tabs, docking and floating rules, window placement, settings pages, status bar, sessions, MCP | all |
| Pages | whiteboard, picture editor, preview (web pages) | all (WebView2 on Windows, WebKit elsewhere) |
| wx shell | draws `core`'s data with wxWidgets | Windows today; macOS and Linux always |
| WinUI shell (new) | draws the same data with WinUI 3 controls | Windows |

Already data in `core` today: `CommandSpec` and the registry, `MenuSpec`, the settings `Page`/`Row`/`Control`, `TabInfo`,
`StatusText`, the workspace's docking and floating rules, `placement` (window placement).

### Rule: WinUI controls as Microsoft provides them

On Windows the shell uses WinUI 3's controls with their own look and default Fluent styling: no restyled templates, no custom-drawn
controls, and none of Note++'s colour themes on WinUI parts (the window follows Windows' light, dark and accent colours; the editor's
colour themes still apply inside Scintilla). Agreed with the owner, 2026-11-09.

Allowed, as that is using controls: setting their normal properties (text, icon, enabled, selection); putting standard controls inside
each other; and arranging standard controls for what WinUI has no single control for (the status bar as text and buttons in a row,
docking as panes with a splitter, tear-out as a new window holding a `TabView`).

What changes on Windows because of the rule:

| Today | Under the rule |
|---|---|
| Coloured tab marks | `TabView` has no colour property: a coloured icon, or dropped |
| Badges on tabs (`wb`, `img`, `enc`, `dock`) | the tab's icon |
| The painted toolbar | a standard `CommandBar`, with Fluent icons |
| The Settings window's layout | a standard `NavigationView` with standard controls; same content, different look |
| Pinned tabs | no built-in pin: an icon, kept first by our own ordering |
| Status bar, Find panel, Markdown toolbar | arrangements of standard controls |

### Rejected: a widget-level abstraction

One interface over "menu", "tab strip", "dock pane" with a WinUI and a wx implementation behind it. Rejected because it can only offer
what both toolkits can do (the WinUI gains are lost), it leaks (focus, keys, layout and event order differ), and every component is
still built twice, plus the abstraction.

## Phases

Each phase ends with `STRICT=1 scripts/check-all.sh` passing and releases continuing on all three systems.

### Phase 0: know that macOS and Linux start

Today both are built on every release and run by nobody.
- A clean-start check on the build servers, as Windows already has: Linux under a virtual display (xvfb), macOS on its runner. A
  release that cannot open its window fails the build.
- Done when: a deliberately broken start fails CI on each system.

### Phase 1: thin `app`, grow `core`

Feature by feature, never all at once. Each step moves one behaviour from `wx/crates/app` into `core`, with unit tests, and leaves
the app working:
1. docking rules and the dock's state as data
2. floating windows (which window, where, what it holds)
3. the status bar's segments and what clicking each does
4. the Settings window's actions (pages are data already)
5. the tab strip's menu and what its items do
6. key routing (which key does what, wherever the keyboard is)

Status (2026-11-09): the six steps are done, each a commit with tests: `core::dock_layout`, floating windows' headings and Dock Back
(`float_headings`, `send_back`, `placement::remember`), `core::status_bar`, `core::settings_pages`, `core::menu` and
`core::context_menus`, `core::key_routing`. Not yet checked with real keys: key routing (posted keys skip wx's key hook on the test
desktop). Still in `app` and worth moving later: the whiteboard and picture pages' traffic, the preview's choice of what to show, the
command handlers' own logic.

Rule, checked in review: shells hold drawing and wiring only, no decisions. `check-architecture.sh` keeps `core` free of any UI toolkit.
Done when: what remains in `app` is drawing and event wiring. This phase is worth doing even if WinUI never happens.

### Phase 2: spike (decides everything after it)

A small separate crate, not shipped: a `windows-reactor` window with a tab strip and a menu bar, Scintilla inside as a child window,
opening and editing a file through `core`. It must show:
- **Focus:** clicking, Tab and F6 into and out of the editor; Ctrl+Tab between tabs. Known pattern: call Win32 `SetFocus` on the editor
  when the host gets focus or is clicked.
- **Alt access keys:** Alt+letter opens the right menu, a bare Alt shows the access keys, F10 works, with the editor focused. Known gap
  in WinUI (WindowsAppSDK #5999); the plan is the routing Note++ already uses for web pages (`wb-menu`, `open_menu_for_key`).
- **Popups above the editor:** WinUI menus and flyouts with `ShouldConstrainToRootBounds="False"` (windowed popups, Windows App SDK
  1.4 and later). Known side effect: no backdrop on those popups.
- **Scintilla's own popups:** autocomplete and calltips beside WinUI flyouts.
- **Resizing and DPI:** moving between monitors of different scale; Scintilla's per-monitor DPI.
- **Drag and drop** between the editor and WinUI parts (a file onto the tab strip).
- **Screen readers:** NVDA and Narrator across WinUI and the Win32 editor (a WinUI window class had a known NVDA gap).
- **Self-contained runtime:** `windows-reactor-setup` stages the Windows App Runtime beside the exe; its size, and that it starts on a
  clean machine with nothing installed.

Done when: each point above works, or has a found fix, and the cost of the rest is known. If the spike fails, phases 3 to 5 stop and
phase 1's gains stay.

Findings so far (2026-11-09, `spike/winui`, windows-reactor 0.100.0 pinned, run on a hidden desktop):
- **Builds with the project's own toolchain** (`x86_64-pc-windows-gnu`) in framework-dependent mode: reactor adds the installed
  `Microsoft.WindowsAppRuntime.2` (2.4 or later) to the process itself, no bootstrap DLL. The window is up in about 250 ms.
- **Self-contained needs another target:** `windows-reactor-setup` supports MSVC and `gnullvm` only. Visual Studio 2022 is on this
  machine, so the MSVC target is a `rustup target add`, not a new toolchain. Not tried yet.
- **Menus from `core` data work** (`MenuSpec` to `MenuBar`). A click reports the item's label, not its key, so labels must be unique
  within the bar. No shortcut text beside items and no `AccessKey` in reactor 0.100.
- **Scintilla as a child window:** created, placed and resized with the layout, and above WinUI's content window in the z-order.
  Reactor exposes neither the window handle nor where an element is, so the window is found among the thread's windows, and the editor
  must be the last row of the layout (its place is the window's bottom-right corner minus the panel's size).
- **Keys:** keys in the editor never reach WinUI, so shortcuts and Ctrl+Tab are handled by `core::key_routing` from the editor's own
  window procedure; with probe messages every route came out right (Ctrl+Tab, Ctrl+Shift+Tab, Alt+F, Alt+E, F10, Ctrl+S, Ctrl+N, a plain
  letter).
- **Blocker for access keys as planned:** reactor 0.100 cannot open a menu from code and does not set `AccessKey`, so Alt+letter and
  F10 are decided but cannot be carried out. Needs a reactor feature, or WinUI calls outside reactor.
- **Reactor quirk:** a tab added and selected in the same turn stays unselected; selecting it in a later publish works.
- **Scintilla must be a layered child window.** As a plain child it was invisible on a real screen: WinUI composes its content over
  the whole window, covering GDI children. Created with `WS_EX_LAYERED` (fully opaque) it gets its own surface and shows; layered
  children need the exe's manifest to declare Windows 8 or later, so the spike embeds Note++'s own `assets/app.manifest`.
- **Checked by the owner on a real screen (2026-11-09):** the editor shows its text; the File and Edit menus open over the editor, not
  behind it; clicking between the tabs and the editor works; Ctrl+Tab and Ctrl+N work while typing in the editor.
- **Left:** Ctrl+S on a real keyboard, Scintilla's own popups (autocomplete, calltips), moving between monitors of different scale,
  drag and drop, screen readers, a self-contained start, and the access keys above.

### Phase 3: the WinUI shell

Draws `core`'s data with standard WinUI controls only: `MenuBar`, `CommandBar`, `TabView`, `NavigationView`, `ContentDialog`,
`InfoBar`, `TreeView`, `ColorPicker`, `ListView`, `AutoSuggestBox`. Custom work is limited to docking and tear-out windows, the editor
host and the glue from data to controls. The whiteboard, picture and preview pages are reused as they are. While it is being built, the
wx shell remains the Windows release.

### Phase 4: lighter wx shell for macOS and Linux

Once Windows ships the WinUI shell, the wx shell drops painted components where a native wx control looks right on GTK and Cocoa,
shrinking `ui-kit`.

### Phase 5: switch Windows over

The WinUI shell becomes the Windows release after a period beside the wx one (both in the zip, or a preview build), with the update path
from the wx version tested on a real install.

## Packaging (Windows)

- **Self-contained Windows App Runtime**, so Note++ starts without anything installed: through `windows-reactor-setup` (the Cargo
  equivalent of the MSBuild switch `WindowsAppSDKSelfContained`).
- The download grows (the runtime is tens of megabytes; to be measured in the spike).
- Runtime security fixes reach users only with a Note++ release: follow Windows App SDK releases.
- More DLLs beside the exe: the manifest pinning and `package.ps1` import check (added in 2.0.8 against planted DLLs) must cover them all.
- One package per processor (x64, later ARM64).

## Risks

| Risk | Weight | Answer |
|---|---|---|
| `windows-reactor` is a preview; its API may change | high | pin a version; the shell is thin, so changes stay small; the spike shows how often it moves |
| Scintilla as a child window misbehaves (focus, popups, DPI) | medium | the spike; fall back to WinUIEdit later (see below) |
| Alt access keys with the editor focused | medium | Note++'s own key routing |
| Two shells drift apart | medium | behaviour only in `core`; shells only draw |
| macOS and Linux break unnoticed | high today | phase 0 start checks; a person on each system before calling them more than experimental |
| Bigger download, runtime to keep updated | low | self-contained, followed with each release |

## Later option: WinUIEdit

[WinUIEdit](https://github.com/BreeceW/WinUIEdit) ports Scintilla into WinUI 3 as a real WinUI control, which would remove the
focus, popup and DPI seams entirely. It exposes Scintilla's message API through `SendMessage`, which fits `core`'s `sci::Send` trait, so
`core` would not change. Not used now: its author says it is not production ready (prerelease 0.0.5, one maintainer), with open
crash, IME and multi-monitor DPI issues. Worth checking again when the WinUI shell exists.

## Decisions needed before each phase

- **New dependencies** (project rule): `windows-rs` and `windows-reactor` for phase 2; whatever the CI start checks need for phase 0.
- **Building on a preview crate** for a shipping product (phase 3).
- **Whether macOS and Linux remain "experimental"** or get a person to test them.
- **When to switch** Windows from the wx shell to the WinUI one (phase 5).

## Research behind this plan

- windows-rs and `windows-reactor`: [microsoft/windows-rs](https://github.com/microsoft/windows-rs/tree/master) ("Declarative UI library
  backed by WinUI 3", a preview).
- Windowed popups: [microsoft-ui-xaml #5958](https://github.com/microsoft/microsoft-ui-xaml/issues/5958), closed as fixed in Windows App
  SDK 1.4; background in [#5942](https://github.com/microsoft/microsoft-ui-xaml/issues/5942).
- Alt access keys with a Win32 control focused: [WindowsAppSDK #5999](https://github.com/microsoft/WindowsAppSDK/issues/5999).
- Focus into a hosted child window: [libghostty-dotnet #2](https://github.com/deblasis/libghostty-dotnet/issues/2) (WPF, same pattern).
- Hosting Win32 child windows: [DesktopChildSiteBridge](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.content.desktopchildsitebridge).
- Popup windows left behind after minimising: [microsoft-ui-xaml #11068](https://github.com/microsoft/microsoft-ui-xaml/issues/11068).
- Scintilla inside WinUI: [WinUIEdit](https://github.com/BreeceW/WinUIEdit) and its [issues](https://github.com/BreeceW/WinUIEdit/issues).
