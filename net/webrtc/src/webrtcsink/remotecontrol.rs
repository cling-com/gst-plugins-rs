// Copyright (C) 2024 Peter Romianowski <pero@cling.com>
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at
// <https://mozilla.org/MPL/2.0/>.
//
// SPDX-License-Identifier: MPL-2.0
use enigo::*;
use once_cell::sync::Lazy;
use std::sync::Once;

static mut GLOBAL_ENIGO: Option<Enigo> = None;
static INIT: Once = Once::new();

fn enigo() -> &'static mut Enigo {
    unsafe {
        INIT.call_once(|| {
            let mut enigo = Enigo::new(&Settings::default()).expect("Failed to create enigo");
            // Release all modifiers. Sometimes we are stuck with a modifier seen as "pressed".
            for key in [
                Key::CapsLock,
                Key::Shift,
                Key::LShift,
                Key::RShift,
                Key::Control,
                Key::LControl,
                Key::RControl,
                Key::Alt,
                Key::Meta,
            ] {
                let _ = enigo.key(key, Direction::Release);
            }
            GLOBAL_ENIGO = Some(enigo);
        });
        GLOBAL_ENIGO.as_mut().unwrap()
    }
}

static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "remotecontrol",
        gst::DebugColorFlags::empty(),
        Some("Remote Control Element"),
    )
});

// Define the RemoteControl struct
#[derive(Default)]
pub struct RemoteControl {}

pub fn handle_remotecontrol_event(event: gst::Event) {
    if let gst::EventView::Navigation(nav_event) = event.view() {
        let structure = nav_event
            .structure()
            .expect("This should be a `Navigation` event");
        let event_name = structure
            .get::<String>("event")
            .expect("`GstNavigation event should have a property `event`");
        match event_name.as_str() {
            "mouse-move" => {
                let x = structure
                    .get::<f64>("pointer_x")
                    .expect("Missing `pointer_x`");
                let y = structure
                    .get::<f64>("pointer_y")
                    .expect("Missing `pointer_y`");
                gst::debug!(CAT, "Mouse moved to ({}, {})", x, y);
                let res = enigo().move_mouse(x.trunc() as i32, y.trunc() as i32, Coordinate::Abs);
                if let Err(err) = res {
                    gst::warning!(CAT, "Mouse move did not succeed: {:?}", err)
                }
                return
            }
            "mouse-button-press" | "mouse-button-release" => {
                gst::debug!(CAT, "Mouse button {}: {:?}", event_name, structure);
                let evt_button = structure.get::<i32>("button").expect("Missing `button`");
                if evt_button >= 1 && evt_button <= 3 {
                    let button = if evt_button == 1 {
                        Button::Left
                    } else if evt_button == 2 {
                        Button::Middle
                    } else {
                        Button::Right
                    };
                    let direction = if event_name == "mouse-button-press" {
                        Direction::Press
                    } else {
                        Direction::Release
                    };
                    let res = enigo().button(button, direction);
                    if let Err(err) = res {
                        gst::warning!(CAT, "Mouse press or release did not succeed: {:?}", err)
                    }
                    return
                }
            }
            "mouse-scroll" => {
                gst::debug!(CAT, "Mouse scroll {:?}", structure);
                let delta_x = structure
                    .get::<f64>("delta_pointer_x")
                    .expect("Missing `delta_pointer_x`") as i32;
                let delta_y = structure
                    .get::<f64>("delta_pointer_y")
                    .expect("Missing `delta_pointer_y`") as i32;
                if delta_x != 0 {
                    let res = enigo().scroll(delta_x, Axis::Horizontal);
                    if let Err(err) = res {
                        gst::warning!(CAT, "Mouse scroll did not succeed: {:?}", err)
                    }
                }
                if delta_y != 0 {
                    let res = enigo().scroll(delta_y, Axis::Vertical);
                    if let Err(err) = res {
                        gst::warning!(CAT, "Mouse scroll did not succeed: {:?}", err)
                    }
                }
            }
            "key-press" | "key-release" => {
                gst::debug!(CAT, "Key press or release {:?}", structure);
                let key_str = structure.get::<String>("key");
                let key = match key_str {
                    Ok(key_str) => {
                        // todo: handle all special keys
                        match key_str.as_str() {
                            "Backspace" => Key::Backspace,
                            "Delete" => Key::Delete,
                            "Tab" => Key::Tab,
                            "Enter" => Key::Return,
                            "Shift" => Key::Shift,
                            "ShiftLeft" => Key::LShift,
                            "ShiftRight" => Key::RShift,
                            "Control" => Key::Control,
                            "ControlLeft" => Key::LControl,
                            "ControlRight" => Key::RControl,
                            "Alt" => Key::Alt,
                            // todo: Enigo does not distinguish between left and right ALT.
                            "AltLeft" => Key::Alt,
                            "AltRight" => Key::Alt,
                            "Meta" => Key::Meta,
                            // todo: Enigo does not distinguish between left and right META.
                            "MetaLeft" => Key::Meta,
                            "MetaRight" => Key::Meta,
                            "CapsLock" => Key::CapsLock,
                            "Escape" => Key::Escape,
                            "Space" => Key::Space,
                            "PageUp" => Key::PageUp,
                            "PageDown" => Key::PageDown,
                            "End" => Key::End,
                            "Home" => Key::Home,
                            "ArrowLeft" => Key::LeftArrow,
                            "ArrowUp" => Key::UpArrow,
                            "ArrowRight" => Key::RightArrow,
                            "ArrowDown" => Key::DownArrow,

                            // Function keys
                            "F1" => Key::F1,
                            "F2" => Key::F2,
                            "F3" => Key::F3,
                            "F4" => Key::F4,
                            "F5" => Key::F5,
                            "F6" => Key::F6,
                            "F7" => Key::F7,
                            "F8" => Key::F8,
                            "F9" => Key::F9,
                            "F10" => Key::F10,
                            "F11" => Key::F11,
                            "F12" => Key::F12,
                            "F13" => Key::F13,
                            "F14" => Key::F14,
                            "F15" => Key::F15,
                            "F16" => Key::F16,
                            "F17" => Key::F17,
                            "F18" => Key::F18,
                            "F19" => Key::F19,
                            "F20" => Key::F20,
                            _ => {
                                let mut chars = key_str.chars();
                                match chars.next() {
                                    Some(c) => {
                                        if chars.next().is_some() {
                                            gst::error!(
                                                CAT,
                                                "Multi-character `key`: {} in {:?}",
                                                key_str,
                                                structure
                                            );
                                            return
                                        }
                                        Key::Unicode(c)
                                    }
                                    None => {
                                        gst::error!(
                                            CAT,
                                            "Empty `key`: {} in {:?}",
                                            key_str,
                                            structure
                                        );
                                        return
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => {
                        gst::warning!(CAT, "`key` not found in: {:?}", structure);
                        return 
                    }
                };
                let direction = if event_name == "key-press" {
                    Direction::Press
                } else {
                    Direction::Release
                };
                // todo: modifiers
                let res = enigo().key(key, direction);
                if let Err(err) = res {
                    gst::warning!(CAT, "Key press or release did not succeed: {:?}", err)
                }
                return
            }
            _ => {
                gst::error!(CAT, "Unhandled navigation event: {:?}", structure);
            }
        }
    } else {
        gst::debug!(CAT, "Not a navigation event: {:?}", event);
    }
}
