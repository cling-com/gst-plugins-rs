// Copyright (C) 2024 Peter Romianowski <pero@cling.com>
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at
// <https://mozilla.org/MPL/2.0/>.
//
// SPDX-License-Identifier: MPL-2.0
use enigo::*;
use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst::StructureRef;
use gst_video::ffi::GstNavigationModifierType;
use gst_video::ffi::GST_NAVIGATION_MODIFIER_SHIFT_MASK;
use gst_video::NavigationModifierType;
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
                enigo.key(key, Direction::Release);
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
pub struct RemoteControl {
    srcpad: gst::Pad,
    sinkpad: gst::Pad,
}

// Implement ObjectSubclass for RemoteControl
#[glib::object_subclass]
impl ObjectSubclass for RemoteControl {
    const NAME: &'static str = "GstRemoteControl";
    type Type = super::RemoteControl;
    type ParentType = gst::Element;

    fn with_class(klass: &Self::Class) -> Self {
        let templ = klass.pad_template("sink").unwrap();
        let sinkpad = gst::Pad::builder_from_template(&templ)
            .chain_function(|pad, parent, buffer| {
                RemoteControl::catch_panic_pad_function(
                    parent,
                    || Err(gst::FlowError::Error),
                    |imp| imp.sink_chain(pad, buffer),
                )
            })
            .event_function(|pad, parent, event| {
                RemoteControl::catch_panic_pad_function(
                    parent,
                    || false,
                    |imp| imp.sink_event(pad, event),
                )
            })
            .query_function(|pad, parent, query| {
                RemoteControl::catch_panic_pad_function(
                    parent,
                    || false,
                    |imp| imp.sink_query(pad, query),
                )
            })
            .build();

        let templ = klass.pad_template("src").unwrap();
        let srcpad = gst::Pad::builder_from_template(&templ)
            .event_function(|pad, parent, event| {
                RemoteControl::catch_panic_pad_function(
                    parent,
                    || false,
                    |imp| imp.src_event(pad, event),
                )
            })
            .query_function(|pad, parent, query| {
                RemoteControl::catch_panic_pad_function(
                    parent,
                    || false,
                    |imp| imp.src_query(pad, query),
                )
            })
            .build();
        Self { srcpad, sinkpad }
    }
}

impl GstObjectImpl for RemoteControl {}

impl ObjectImpl for RemoteControl {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.add_pad(&self.sinkpad).unwrap();
        obj.add_pad(&self.srcpad).unwrap();
    }
}

// Implement ElementImpl for RemoteControl
impl ElementImpl for RemoteControl {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "Remote Control",
                "Generic/Filter",
                "Listens for GstNavigation events and performs them using enigo",
                "Peter Romianowski <pero@cling.com>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
            let caps = gst::Caps::new_any();
            let src_pad_template = gst::PadTemplate::new(
                "src",
                gst::PadDirection::Src,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            let sink_pad_template = gst::PadTemplate::new(
                "sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &caps,
            )
            .unwrap();

            vec![sink_pad_template, src_pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }

    fn change_state(
        &self,
        transition: gst::StateChange,
    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        gst::trace!(CAT, imp = self, "Changing state {:?}", transition);
        self.parent_change_state(transition)
    }
}

// Implement the `Navigation` interface for RemoteControl
impl RemoteControl {
    fn sink_chain(
        &self,
        pad: &gst::Pad,
        buffer: gst::Buffer,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        gst::debug!(CAT, obj = pad, "sink_chain: {:?}", buffer);
        self.srcpad.push(buffer)
    }

    fn sink_event(&self, pad: &gst::Pad, event: gst::Event) -> bool {
        gst::debug!(CAT, obj = pad, "sink_event: {:?}", event);
        if let gst::EventView::Navigation(nav_event) = event.view() {
            gst::info!(
                CAT,
                obj = pad,
                "Received navigation event: {:?}",
                nav_event.structure()
            );
        }

        // Forward the event to the source pad
        self.srcpad.push_event(event);
        return true;
    }

    fn sink_query(&self, pad: &gst::Pad, query: &mut gst::QueryRef) -> bool {
        gst::debug!(CAT, obj = pad, "sink_query: {:?}", query);
        self.srcpad.peer_query(query)
    }

    fn src_event(&self, pad: &gst::Pad, event: gst::Event) -> bool {
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
                    gst::debug!(CAT, obj = pad, "Mouse moved to ({}, {})", x, y);
                    enigo().move_mouse(x.trunc() as i32, y.trunc() as i32, Coordinate::Abs);
                    return true;
                }
                "mouse-button-press" | "mouse-button-release" => {
                    gst::error!(
                        CAT,
                        obj = pad,
                        "Mouse button {}: {:?}",
                        event_name,
                        structure
                    );
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
                        enigo().button(button, direction);
                        return true;
                    }
                }
                "mouse-scroll" => {
                    gst::error!(CAT, obj = pad, "Mouse scroll {:?}", structure);
                    let delta_x = structure
                        .get::<f64>("delta_pointer_x")
                        .expect("Missing `delta_pointer_x`")
                        as i32;
                    let delta_y = structure
                        .get::<f64>("delta_pointer_y")
                        .expect("Missing `delta_pointer_y`")
                        as i32;
                    if delta_x != 0 {
                        enigo().scroll(delta_x, Axis::Horizontal);
                    }
                    if delta_y != 0 {
                        enigo().scroll(delta_y, Axis::Vertical);
                    }
                }
                "key-press" | "key-release" => {
                    gst::error!(CAT, obj = pad, "Key something {:?}", structure);
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
                                                    obj = pad,
                                                    "Multi-character `key`: {} in {:?}",
                                                    key_str,
                                                    structure
                                                );
                                                return true;
                                            }
                                            Key::Unicode(c)
                                        }
                                        None => {
                                            gst::error!(
                                                CAT,
                                                obj = pad,
                                                "Empty `key`: {} in {:?}",
                                                key_str,
                                                structure
                                            );
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            gst::error!(CAT, obj = pad, "`key` not found in: {:?}", structure);
                            return true;
                        }
                    };
                    let direction = if event_name == "key-press" {
                        Direction::Press
                    } else {
                        Direction::Release
                    };
                    gst::error!(CAT, obj = pad, "Key '{:?}' {:?}", key, direction);
                    // todo: modifiers
                    let res = enigo().key(key, direction);
                    match res {
                        Ok(_) => {}
                        Err(e) => {
                            gst::error!(CAT, obj = pad, "Failed to send key event: {:?}", e);
                        }
                    }
                    return true;
                }
                _ => {
                    gst::error!(
                        CAT,
                        obj = pad,
                        "Unhandled navigation event: {:?}",
                        structure
                    );
                }
            }
        } else {
            gst::error!(CAT, obj = pad, "Not a navigation event: {:?}", event);
        }
        // Forward the event to the sink pad
        self.sinkpad.push_event(event)
    }

    fn src_query(&self, pad: &gst::Pad, query: &mut gst::QueryRef) -> bool {
        gst::debug!(CAT, obj = pad, "src_query: {:?}", query);
        // todo: do we need this?
        self.sinkpad.peer_query(query)
    }
}
