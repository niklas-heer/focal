//! The menu bar. Items dispatch the same actions as the keyboard shortcuts,
//! which macOS shows beside them.

use gpui_kit::{App, Menu, MenuItem, OsAction, actions};

use crate::cli_install::{InstallCommand, UninstallCommand};
use crate::editor::{
    Bold, CloseWindow, Copy, Cut, InlineCode, InsertCodeBlock, InsertLink, InsertMath, InsertTable,
    Italic, OpenLink, Paste, Quit, Redo, Save, SelectAll, SetHeading, Strikethrough, ToggleBullets,
    ToggleFocusMode, ToggleNumbers, ToggleQuote, ToggleTask, Undo,
};
use crate::find_bar::{Find, FindAndReplace, FindNext, FindPrevious};
use crate::settings::OpenSettings;
use crate::switcher::QuickOpen;
use crate::updates::{CheckForUpdates, OpenReleases};
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

pub fn set_menus(cx: &mut App) {
    cx.on_action(|_: &AboutFocal, _| crate::mac::about_panel());
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
            MenuItem::action("Save", Save),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            MenuItem::separator(),
            MenuItem::submenu(Menu::new("Find").items([
                MenuItem::action("Find…", Find),
                MenuItem::action("Find and Replace…", FindAndReplace),
                MenuItem::action("Find Next", FindNext),
                MenuItem::action("Find Previous", FindPrevious),
            ])),
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
            MenuItem::action("Focus Mode", ToggleFocusMode),
            MenuItem::separator(),
            MenuItem::action("Open Link", OpenLink),
            MenuItem::action("Back", GoBack),
        ]),
    ]);
}
