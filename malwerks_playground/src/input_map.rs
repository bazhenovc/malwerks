// Copyright (c) 2020-2021 Kyrylo Bazhenov
//
// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0.
// If a copy of the MPL was not distributed with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

#[derive(Copy, Clone)]
pub enum InputActionType {
    CameraMove,
    CameraStrafe,
    CameraLift,
    CameraRotateX,
    CameraRotateY,
}

#[derive(Copy, Clone)]
pub struct InputAction {
    pub action_type: InputActionType,
    pub action_value: f32,
}

pub struct InputMap {
    keyboard_map: HashMap<winit::event::VirtualKeyCode, KeyboardActionState>,
    gamepad_axis_map: HashMap<gilrs::Axis, GamepadAxisState>,
    gamepad_button_map: HashMap<gilrs::Button, GamepadButtonState>,

    mouse_drag_map: HashMap<winit::event::MouseButton, MouseDragActionState>,
    mouse_position: winit::dpi::PhysicalPosition<f64>,
    mouse_anchor: winit::dpi::PhysicalPosition<f64>,
    mouse_grab: bool,

    action_queue: Vec<InputAction>,
}

impl InputMap {
    pub fn new() -> Self {
        Self {
            keyboard_map: HashMap::new(),
            gamepad_axis_map: HashMap::new(),
            gamepad_button_map: HashMap::new(),
            mouse_drag_map: HashMap::new(),

            mouse_position: winit::dpi::PhysicalPosition::new(0.0, 0.0),
            mouse_anchor: winit::dpi::PhysicalPosition::new(0.0, 0.0),
            mouse_grab: false,

            action_queue: Vec::new(),
        }
    }
    pub fn bind_keyboard(
        &mut self,
        key: winit::event::VirtualKeyCode,
        action_type: InputActionType,
        on_pressed: f32,
        on_released: f32,
    ) {
        self.keyboard_map
            .insert(key, KeyboardActionState::new(action_type, on_pressed, on_released));
    }

    pub fn bind_gamepad_axis(&mut self, axis: gilrs::Axis, action_type: InputActionType, scale: f32) {
        self.gamepad_axis_map
            .insert(axis, GamepadAxisState::new(action_type, 0.0, scale));
    }

    pub fn bind_gamepad_button(
        &mut self,
        button: gilrs::Button,
        action_type: InputActionType,
        on_pressed: f32,
        on_released: f32,
    ) {
        self.gamepad_button_map
            .insert(button, GamepadButtonState::new(action_type, on_pressed, on_released));
    }

    pub fn bind_mouse_drag(
        &mut self,
        button: winit::event::MouseButton,
        action_type0: InputActionType,
        action_type1: InputActionType,
    ) {
        self.mouse_drag_map
            .insert(button, MouseDragActionState::new(action_type0, action_type1));
    }

    pub fn handle_event<T>(
        &mut self,
        keyboard_captured: bool,
        mouse_captured: bool,
        window: &winit::window::Window,
        event: &winit::event::Event<T>,
    ) {
        use winit::event::*;

        if !keyboard_captured {
            if let Event::WindowEvent { event, .. } = event {
                if let WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(keycode),
                            state,
                            ..
                        },
                    ..
                } = event
                {
                    if let Some(action) = self.keyboard_map.get_mut(&keycode) {
                        action.active = true;
                        action.state = *state;
                    }
                }
            }
        }

        if !mouse_captured {
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        self.mouse_position = *position;
                        if self.mouse_grab {
                            match window.set_cursor_position(self.mouse_anchor) {
                                Err(e) => {
                                    log::warn!(
                                        "failed to set mouse cursor position: {:?}, actions will not be processed",
                                        e
                                    );
                                }
                                _ => {
                                    let delta_position = (
                                        self.mouse_anchor.x - self.mouse_position.x,
                                        self.mouse_position.y - self.mouse_anchor.y,
                                    );
                                    for action in self.mouse_drag_map.values_mut() {
                                        action.value0 = delta_position.0 as _;
                                        action.value1 = delta_position.1 as _;
                                    }
                                }
                            }
                        }
                    }

                    WindowEvent::MouseInput {
                        button,
                        state: ElementState::Pressed,
                        ..
                    } => {
                        if let Some(action) = self.mouse_drag_map.get_mut(button) {
                            action.active = true;
                            self.mouse_anchor = self.mouse_position;
                            self.mouse_grab = true;
                            window.set_cursor_visible(false);
                            if let Err(error) = window.set_cursor_grab(true) {
                                log::warn!("failed to grab mouse cursor: {:?}", error);
                            }
                        }
                    }

                    WindowEvent::MouseInput {
                        button,
                        state: ElementState::Released,
                        ..
                    } => {
                        if let Some(action) = self.mouse_drag_map.get_mut(button) {
                            action.active = false;
                            action.value0 = 0.0;
                            action.value1 = 0.0;
                            if self.mouse_grab {
                                self.mouse_grab = false;
                                window.set_cursor_visible(true);
                                if let Err(error) = window.set_cursor_grab(false) {
                                    log::warn!("failed to release cursor grab: {:?}", error);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn handle_gamepad_event(&mut self, event: &gilrs::Event) {
        match event.event {
            gilrs::EventType::AxisChanged(axis, value, _) => {
                if let Some(action) = self.gamepad_axis_map.get_mut(&axis) {
                    action.active = true;
                    action.value = value;
                }
            }
            gilrs::EventType::ButtonPressed(button, _) => {
                if let Some(action) = self.gamepad_button_map.get_mut(&button) {
                    action.active = true;
                    action.state = true;
                }
            }
            gilrs::EventType::ButtonReleased(button, _) => {
                if let Some(action) = self.gamepad_button_map.get_mut(&button) {
                    action.active = true;
                    action.state = false;
                }
            }

            _ => {}
        }
    }

    pub fn process_events(&mut self) {
        self.action_queue.clear();

        for action in self.keyboard_map.values_mut() {
            if action.active {
                self.action_queue.push(InputAction {
                    action_type: action.action_type,
                    action_value: action.consume_value(),
                });
            }
        }
        for action in self.mouse_drag_map.values_mut() {
            if action.active {
                self.action_queue.push(InputAction {
                    action_type: action.action_type0,
                    action_value: action.value0,
                });
                self.action_queue.push(InputAction {
                    action_type: action.action_type1,
                    action_value: action.value1,
                });
            }
        }
        for action in self.gamepad_axis_map.values_mut() {
            if action.active {
                self.action_queue.push(InputAction {
                    action_type: action.action_type,
                    action_value: action.consume_value(),
                });
            }
        }
        for action in self.gamepad_button_map.values_mut() {
            if action.active {
                self.action_queue.push(InputAction {
                    action_type: action.action_type,
                    action_value: action.consume_value(),
                });
            }
        }
    }

    pub fn get_action_queue(&self) -> &[InputAction] {
        &self.action_queue
    }
}

struct KeyboardActionState {
    action_type: InputActionType,
    active: bool,
    state: winit::event::ElementState,
    on_pressed: f32,
    on_released: f32,
}

impl KeyboardActionState {
    fn new(action_type: InputActionType, on_pressed: f32, on_released: f32) -> Self {
        Self {
            action_type,
            active: false,
            state: winit::event::ElementState::Released,
            on_pressed,
            on_released,
        }
    }

    fn consume_value(&mut self) -> f32 {
        if self.state == winit::event::ElementState::Pressed {
            self.on_pressed
        } else {
            self.active = false;
            self.on_released
        }
    }
}

struct MouseDragActionState {
    action_type0: InputActionType,
    action_type1: InputActionType,
    active: bool,
    value0: f32,
    value1: f32,
}

impl MouseDragActionState {
    fn new(action_type0: InputActionType, action_type1: InputActionType) -> Self {
        Self {
            action_type0,
            action_type1,
            active: false,
            value0: 0.0,
            value1: 0.0,
        }
    }
}

struct GamepadButtonState {
    action_type: InputActionType,
    active: bool,
    state: bool,
    on_pressed: f32,
    on_released: f32,
}

impl GamepadButtonState {
    fn new(action_type: InputActionType, on_pressed: f32, on_released: f32) -> Self {
        Self {
            action_type,
            active: false,
            state: false,
            on_pressed,
            on_released,
        }
    }

    fn consume_value(&mut self) -> f32 {
        if self.state {
            self.on_pressed
        } else {
            self.active = false;
            self.on_released
        }
    }
}

struct GamepadAxisState {
    action_type: InputActionType,
    active: bool,
    value: f32,
    scale: f32,
}

impl GamepadAxisState {
    fn new(action_type: InputActionType, value: f32, scale: f32) -> Self {
        Self {
            action_type,
            active: false,
            value,
            scale,
        }
    }

    fn consume_value(&mut self) -> f32 {
        const DEAD_ZONE: f32 = 0.15;
        self.active = self.value >= DEAD_ZONE;
        self.value * self.scale
    }
}
