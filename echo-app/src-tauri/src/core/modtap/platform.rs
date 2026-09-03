//! Per-platform key-state reads for [`super::ModTapWatcher`].
//!
//! Two questions only: is the watched modifier down, and is anything else down.
//! Nothing here can observe *which* key was typed beyond that yes/no, and the
//! second question is only ever asked while the modifier is held.

use super::ModifierKey;

/// A live handle to whatever the platform needs to answer those two questions.
/// Owned by the watcher thread, so it never has to be `Send`.
pub struct KeyReader {
    #[cfg(target_os = "linux")]
    display: *mut x11::xlib::Display,
    #[cfg(target_os = "linux")]
    watched: Vec<u8>,
}

/* ---- Windows -------------------------------------------------------------- */

#[cfg(target_os = "windows")]
mod imp {
    use super::super::ModifierKey;

    /// Virtual-key codes that count as "the watched modifier", including the
    /// side-specific ones: Windows reports both the generic and the left/right
    /// code, and either may be the one that is set.
    fn vks(key: ModifierKey) -> &'static [i32] {
        match key {
            ModifierKey::Control => &[0x11, 0xA2, 0xA3], // VK_CONTROL, L, R
            ModifierKey::Alt => &[0x12, 0xA4, 0xA5],     // VK_MENU, L, R
            ModifierKey::Shift => &[0x10, 0xA0, 0xA1],   // VK_SHIFT, L, R
            ModifierKey::Meta => &[0x5B, 0x5C],          // VK_LWIN, VK_RWIN
        }
    }

    fn down(vk: i32) -> bool {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
        // The high bit is "currently down"; the low bit is "pressed since last
        // call", which we must not treat as held.
        (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
    }

    pub fn modifier_down(key: ModifierKey) -> bool {
        vks(key).iter().any(|vk| down(*vk))
    }

    pub fn other_key_down(key: ModifierKey) -> bool {
        let own = vks(key);
        // 0x01..0x06 are the mouse buttons. A Ctrl+click is as much "not a bare
        // tap" as a Ctrl+C is, so they cancel too.
        (0x01..=0xFE).any(|vk| !own.contains(&vk) && down(vk))
    }
}

/* ---- macOS ---------------------------------------------------------------- */

#[cfg(target_os = "macos")]
mod imp {
    use super::super::ModifierKey;

    // Declared here rather than taken from core-graphics' Rust bindings: these
    // two C entry points are stable, and the binding surface for them is not.
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceFlagsState(state: i32) -> u64;
        fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    }

    /// kCGEventSourceStateCombinedSessionState — the whole session, not just
    /// events this process posted.
    const COMBINED: i32 = 0;

    /// CGEventFlags bits for each modifier.
    fn flag(key: ModifierKey) -> u64 {
        match key {
            ModifierKey::Control => 0x0004_0000, // kCGEventFlagMaskControl
            ModifierKey::Alt => 0x0008_0000,     // kCGEventFlagMaskAlternate
            ModifierKey::Shift => 0x0002_0000,   // kCGEventFlagMaskShift
            ModifierKey::Meta => 0x0010_0000,    // kCGEventFlagMaskCommand
        }
    }

    /// Virtual keycodes of the modifier keys themselves, which must not count
    /// as "some other key".
    const MODIFIER_KEYCODES: &[u16] = &[
        54, 55, // Command right, left
        56, 60, // Shift left, right
        58, 61, // Option left, right
        59, 62, // Control left, right
        57, 63, // Caps Lock, Fn
    ];

    pub fn modifier_down(key: ModifierKey) -> bool {
        unsafe { CGEventSourceFlagsState(COMBINED) & flag(key) != 0 }
    }

    pub fn other_key_down(_key: ModifierKey) -> bool {
        // Keycodes stop at 0x7F on macOS.
        (0u16..0x80).any(|k| {
            !MODIFIER_KEYCODES.contains(&k) && unsafe { CGEventSourceKeyState(COMBINED, k) }
        })
    }
}

/* ---- Linux (X11) ---------------------------------------------------------- */

#[cfg(target_os = "linux")]
mod imp {
    use super::super::ModifierKey;

    /// Keysyms for both sides of each modifier.
    fn keysyms(key: ModifierKey) -> [u64; 2] {
        match key {
            ModifierKey::Control => [x11::keysym::XK_Control_L, x11::keysym::XK_Control_R],
            ModifierKey::Alt => [x11::keysym::XK_Alt_L, x11::keysym::XK_Alt_R],
            ModifierKey::Shift => [x11::keysym::XK_Shift_L, x11::keysym::XK_Shift_R],
            ModifierKey::Meta => [x11::keysym::XK_Super_L, x11::keysym::XK_Super_R],
        }
        .map(u64::from)
    }

    /// The keycodes X assigns to `key` on this keyboard. Keycodes are not fixed
    /// across layouts, so they have to be looked up rather than hard-coded.
    pub fn watched_keycodes(display: *mut x11::xlib::Display, key: ModifierKey) -> Vec<u8> {
        keysyms(key)
            .iter()
            .map(|sym| unsafe { x11::xlib::XKeysymToKeycode(display, *sym) })
            .filter(|code| *code != 0)
            .collect()
    }

    /// The full 256-bit "which keys are down" bitmap, in one round trip.
    fn keymap(display: *mut x11::xlib::Display) -> [i8; 32] {
        let mut keys = [0i8; 32];
        unsafe { x11::xlib::XQueryKeymap(display, keys.as_mut_ptr()) };
        keys
    }

    fn is_set(keys: &[i8; 32], code: u8) -> bool {
        keys[(code / 8) as usize] as u8 & (1 << (code % 8)) != 0
    }

    pub fn modifier_down(display: *mut x11::xlib::Display, watched: &[u8]) -> bool {
        let keys = keymap(display);
        watched.iter().any(|c| is_set(&keys, *c))
    }

    pub fn other_key_down(display: *mut x11::xlib::Display, watched: &[u8]) -> bool {
        let keys = keymap(display);
        (0u16..256).any(|c| {
            let code = c as u8;
            !watched.contains(&code) && is_set(&keys, code)
        })
    }
}

/* ---- Shared surface ------------------------------------------------------- */

impl KeyReader {
    /// `None` when this platform cannot answer, so the caller can fall back to
    /// telling the user that bare modifiers are unavailable here.
    #[cfg(not(target_os = "linux"))]
    pub fn open(_key: ModifierKey) -> Option<Self> {
        Some(Self {})
    }

    #[cfg(target_os = "linux")]
    pub fn open(key: ModifierKey) -> Option<Self> {
        // No X display means Wayland or a headless session; neither can be
        // polled this way.
        let display = unsafe { x11::xlib::XOpenDisplay(std::ptr::null()) };
        if display.is_null() {
            return None;
        }
        let watched = imp::watched_keycodes(display, key);
        if watched.is_empty() {
            unsafe { x11::xlib::XCloseDisplay(display) };
            return None;
        }
        Some(Self { display, watched })
    }

    #[cfg(not(target_os = "linux"))]
    pub fn modifier_down(&self, key: ModifierKey) -> bool {
        imp::modifier_down(key)
    }

    #[cfg(target_os = "linux")]
    pub fn modifier_down(&self, _key: ModifierKey) -> bool {
        imp::modifier_down(self.display, &self.watched)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn other_key_down(&self, key: ModifierKey) -> bool {
        imp::other_key_down(key)
    }

    #[cfg(target_os = "linux")]
    pub fn other_key_down(&self, _key: ModifierKey) -> bool {
        imp::other_key_down(self.display, &self.watched)
    }
}

#[cfg(target_os = "linux")]
impl Drop for KeyReader {
    fn drop(&mut self) {
        unsafe { x11::xlib::XCloseDisplay(self.display) };
    }
}
