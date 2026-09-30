//! The menu bar. Items dispatch the same actions as the keyboard shortcuts,
//! which macOS shows beside them.

use gpui_kit::{App, Menu, MenuItem, OsAction};

use crate::editor::{
    Bold, CloseWindow, Copy, Cut, Italic, Paste, Quit, Redo, Save, SelectAll, ToggleFocusMode, Undo,
};
use crate::settings::OpenSettings;

pub fn set_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu::new("Focal").items([
            MenuItem::action("Settings…", OpenSettings),
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
        ]),
        Menu::new("Format").items([
            MenuItem::action("Bold", Bold),
            MenuItem::action("Italic", Italic),
        ]),
        Menu::new("View").items([MenuItem::action("Focus Mode", ToggleFocusMode)]),
    ]);
}
