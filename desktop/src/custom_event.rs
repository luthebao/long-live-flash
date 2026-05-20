//! Custom event type for desktop ruffle

use llflash_core::events::PlayerNotification;
use llflash_frontend_utils::content::ContentDescriptor;

use crate::gui::DialogDescriptor;
use crate::player::{LaunchOptions, PlayerRunnable};

pub enum OpenType {
    File,
    Directory,
}

/// User-defined events.
pub enum RuffleEvent {
    /// Indicates that a task is ready to be polled.
    TaskPoll(PlayerRunnable),

    /// Indicates that an asynchronous SWF metadata load has been completed.
    OnMetadata(llflash_core::swf::HeaderExt),

    /// The user requested to pick and then open a file.
    BrowseAndOpen(Box<LaunchOptions>, OpenType),

    /// The user requested to open a movie.
    Open(ContentDescriptor, Box<LaunchOptions>),

    /// The user requested to close the current SWF.
    CloseFile,

    /// The user requested to enter full screen.
    EnterFullScreen,

    /// The user requested to exit full screen.
    ExitFullScreen,

    /// The user requested to exit Llflash.
    ExitRequested,

    /// The user selected an item in the right-click context menu.
    ContextMenuItemClicked(usize),

    /// The movie wants to open a dialog.
    OpenDialog(DialogDescriptor),

    /// Llflash core has a notification to handle.
    PlayerNotification(PlayerNotification),

    /// Export Llflash Bundle from currently playing content and open save dialog.
    ExportBundle,

    /// Re-apply the cached `ime_allowed` state on the main window.
    ///
    /// macOS NSOpenPanel can leave the window's IME hook detached after the
    /// sheet closes; sending this after a file picker resolves restores text
    /// input. Mirrors the SDL3 `StartTextInput` re-arm in the Odin host.
    ReArmIme,
}
