//! Parse Linux input_event from raw bytes (reMarkable is 32-bit ARM: 16 bytes per event).

use evdevil::event::{EventType, InputEvent};

/// Size of struct input_event on 32-bit ARM (timeval 8 + type 2 + code 2 + value 4).
pub const INPUT_EVENT_SIZE: usize = 16;

/// Parse one input_event from buffer (little-endian, 16 bytes).
/// Returns None if buffer is too short.
pub fn parse_input_event(buf: &[u8]) -> Option<InputEvent> {
    if buf.len() < INPUT_EVENT_SIZE {
        return None;
    }
    let ty = u16::from_le_bytes([buf[8], buf[9]]);
    let code = u16::from_le_bytes([buf[10], buf[11]]);
    let value = i32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    Some(InputEvent::new(EventType::from_raw(ty), code, value))
}
