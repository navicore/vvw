//! Keyboard input handling: WASD + arrow keys

use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use web_sys::KeyboardEvent;

/// Tracks which movement keys are currently pressed
#[allow(clippy::struct_excessive_bools)]
pub struct InputState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl InputState {
    fn new() -> Self {
        Self {
            up: false,
            down: false,
            left: false,
            right: false,
        }
    }
}

/// Register keydown/keyup listeners and return shared input state.
/// The returned closures must be kept alive for the duration of the game.
#[allow(clippy::type_complexity)]
pub fn setup_input() -> Result<
    (
        Rc<RefCell<InputState>>,
        Vec<Closure<dyn FnMut(KeyboardEvent)>>,
    ),
    JsValue,
> {
    let state = Rc::new(RefCell::new(InputState::new()));
    let document = web_sys::window()
        .ok_or("no window")?
        .document()
        .ok_or("no document")?;

    let mut closures = Vec::new();

    // keydown
    {
        let state = Rc::clone(&state);
        let closure = Closure::wrap(Box::new(move |event: KeyboardEvent| {
            update_key(&state, &event.code(), true);
        }) as Box<dyn FnMut(KeyboardEvent)>);
        document.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
        closures.push(closure);
    }

    // keyup
    {
        let state = Rc::clone(&state);
        let closure = Closure::wrap(Box::new(move |event: KeyboardEvent| {
            update_key(&state, &event.code(), false);
        }) as Box<dyn FnMut(KeyboardEvent)>);
        document.add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref())?;
        closures.push(closure);
    }

    Ok((state, closures))
}

fn update_key(state: &Rc<RefCell<InputState>>, code: &str, pressed: bool) {
    let mut s = state.borrow_mut();
    match code {
        "KeyW" | "ArrowUp" => s.up = pressed,
        "KeyS" | "ArrowDown" => s.down = pressed,
        "KeyA" | "ArrowLeft" => s.left = pressed,
        "KeyD" | "ArrowRight" => s.right = pressed,
        _ => {}
    }
}
