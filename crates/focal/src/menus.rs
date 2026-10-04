//! The menu bar. Items dispatch the same actions as the keyboard shortcuts,
//! which macOS shows beside them.

use gpui_kit::{App, Menu, MenuItem, OsAction, actions};

use crate::cli_install::{InstallCommand, UninstallCommand};
use crate::editor::{
    Bold, CloseWindow, Copy, CopyHtml, Cut, ExportHtml, ExportPdf, InlineCode, InsertCodeBlock,
    InsertLink, InsertMath, InsertTable, Italic, OpenLink, Paste, Print, Quit, Redo, Save,
    SelectAll, SetHeading, Strikethrough, ToggleBullets, ToggleFocusMode, ToggleNumbers,
    ToggleQuote, ToggleTask, Undo,
};
use crate::find_bar::{Find, FindAndReplace, FindNext, FindPrevious};
use crate::settings::{OpenSettings, Settings, ToggleVimMode};
use crate::switcher::{GoToHeading, QuickOpen};
use crate::updates::{CheckForUpdates, OpenReleases};
use crate::windows::{NewWindow, OpenFiles};
use crate::workspace::{GoBack, ToggleSidebar};

actions!(focal, [AboutFocal]);

/// "Check for Updates…" when this Focal can update itself, otherwise a link
/// to the releases.
pub fn update_item(cx: &App) -> (&'static str, MenuItem) {
    if crate::updates::available(cx) {
        let label = "Check for Updates…";
        (label, MenuItem::action(label, CheckForUpdates))
    } else {
        let label = "Focal Releases…";
        (label, MenuItem::action(label, OpenReleases))
    }
}

pub fn init(cx: &mut App) {
    cx.on_action(|_: &AboutFocal, cx| crate::about::open_window(cx));
    set_menus(cx);
    // Vim Mode's checkmark follows the setting.
    let mut vim_mode = cx.global::<Settings>().vim_mode;
    cx.observe_global::<Settings>(move |cx| {
        let now = cx.global::<Settings>().vim_mode;
        if now != vim_mode {
            vim_mode = now;
            set_menus(cx);
        }
    })
    .detach();
}

/// Builds the menu bar, again whenever Open Recent changes.
pub fn set_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu::new("Focal").items([
            MenuItem::action("About Focal", AboutFocal),
            update_item(cx).1,
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::action("Install Command Line Tool…", InstallCommand),
            MenuItem::action("Uninstall Command Line Tool…", UninstallCommand),
            MenuItem::separator(),
            MenuItem::action("Quit Focal", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("New", NewWindow),
            MenuItem::action("Open…", OpenFiles),
            MenuItem::submenu(crate::recent::menu(cx)),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
            MenuItem::action("Export as HTML…", ExportHtml),
            MenuItem::action("Export as PDF…", ExportPdf),
            MenuItem::separator(),
            MenuItem::action("Print…", Print),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::action("Copy as HTML", CopyHtml),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            MenuItem::separator(),
            MenuItem::submenu(Menu::new("Find").items([
                MenuItem::action("Find…", Find),
                MenuItem::action("Find and Replace…", FindAndReplace),
                MenuItem::action("Find Next", FindNext),
                MenuItem::action("Find Previous", FindPrevious),
            ])),
            MenuItem::separator(),
            MenuItem::action("Vim Mode", ToggleVimMode).checked(cx.global::<Settings>().vim_mode),
        ]),
        Menu::new("Format").items([
            MenuItem::action("Bold", Bold),
            MenuItem::action("Italic", Italic),
            MenuItem::action("Strikethrough", Strikethrough),
            MenuItem::action("Inline Code", InlineCode),
            MenuItem::action("Link", InsertLink),
            MenuItem::separator(),
            MenuItem::action("Paragraph", SetHeading(0)),
            MenuItem::action("Heading 1", SetHeading(1)),
            MenuItem::action("Heading 2", SetHeading(2)),
            MenuItem::action("Heading 3", SetHeading(3)),
            MenuItem::separator(),
            MenuItem::action("Bulleted List", ToggleBullets),
            MenuItem::action("Numbered List", ToggleNumbers),
            MenuItem::action("Task", ToggleTask),
            MenuItem::action("Quote", ToggleQuote),
            MenuItem::separator(),
            MenuItem::action("Table", InsertTable),
            MenuItem::action("Code Block", InsertCodeBlock),
            MenuItem::action("Math Block", InsertMath),
        ]),
        Menu::new("View").items([
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::action("Quick Open…", QuickOpen),
            MenuItem::action("Go to Heading…", GoToHeading),
            MenuItem::action("Focus Mode", ToggleFocusMode),
            MenuItem::separator(),
            MenuItem::action("Open Link", OpenLink),
            MenuItem::action("Back", GoBack),
        ]),
    ]);
}
