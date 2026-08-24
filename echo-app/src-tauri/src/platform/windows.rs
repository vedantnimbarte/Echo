use crate::core::injection::TextInjector;
use crate::error::{EchoError, Result};

pub struct WindowsInjector;

impl WindowsInjector {
    pub fn new() -> Self {
        Self
    }
}

impl TextInjector for WindowsInjector {
    fn inject_text(&self, text: &str) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::Input::KeyboardAndMouse::{
                SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
                VIRTUAL_KEY,
            };

            let mut inputs: Vec<INPUT> = Vec::with_capacity(text.len() * 2);

            for ch in text.encode_utf16() {
                let ki_down = KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: ch,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                };
                let ki_up = KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: ch,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                };

                unsafe {
                    inputs.push(INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                            ki: ki_down,
                        },
                    });
                    inputs.push(INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                            ki: ki_up,
                        },
                    });
                }
            }

            let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
            if sent != inputs.len() as u32 {
                return Err(EchoError::Injection(
                    "SendInput did not process all events".into(),
                ));
            }

            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        Err(EchoError::Injection(
            "Windows injector called on non-Windows platform".into(),
        ))
    }

    fn send_paste(&self) -> Result<()> {
        send_ctrl_chord(VK_V, "paste")
    }

    fn send_copy(&self) -> Result<()> {
        send_ctrl_chord(VK_C, "copy")
    }
}

const VK_CONTROL: u16 = 0x11;
const VK_C: u16 = 0x43;
const VK_V: u16 = 0x56;

/// Send Ctrl+`vk` as a four-event chord (Ctrl↓ key↓ key↑ Ctrl↑).
/// `label` only names the shortcut in the error message.
fn send_ctrl_chord(vk: u16, label: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
        };

        fn key(vk: u16, up: bool) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: 0,
                        dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            }
        }

        let inputs = [
            key(VK_CONTROL, false),
            key(vk, false),
            key(vk, true),
            key(VK_CONTROL, true),
        ];
        let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent != inputs.len() as u32 {
            return Err(EchoError::Injection(format!(
                "SendInput did not process the {label} shortcut"
            )));
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (vk, label);
        Err(EchoError::Injection(
            "Windows injector called on non-Windows platform".into(),
        ))
    }
}
