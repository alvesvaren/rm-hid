//! Forward reMarkable touch as a relative touchpad (cursor + finger down/up).
//! Converts absolute MT positions to REL_X/REL_Y deltas; only BTN_TOUCH for contact (no BTN_LEFT).

use std::io::Read;
use std::path::Path;

use evdevil::event::{InputEvent, Key, Rel};
use evdevil::uinput::UinputDevice;
use evdevil::InputProp;

use crate::config::TOUCH_DEVICE;
use crate::event::{
    key_event, parse_input_event, rel_event, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_MT_SLOT,
    ABS_MT_TRACKING_ID, EV_ABS, EV_KEY, EV_SYN, INPUT_EVENT_SIZE, REL_X, REL_Y, SYN_REPORT,
};
use crate::ssh;

fn create_touchpad_device() -> Result<UinputDevice, Box<dyn std::error::Error + Send + Sync>> {
    // Kernel uinput docs: virtual mouse must declare BTN_LEFT so REL_X/REL_Y move the cursor.
    // We only emit BTN_TOUCH for touch down/up; BTN_LEFT is never sent (avoids stuck-button).
    let device = UinputDevice::builder()?
        .with_props([InputProp::POINTER])?
        .with_rel_axes([Rel::X, Rel::Y])?
        .with_keys([Key::BTN_LEFT, Key::BTN_TOUCH])?
        .build("reMarkable Touch")?;
    Ok(device)
}

pub fn run(key_path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_sess, mut channel) = ssh::open_input_stream(TOUCH_DEVICE, key_path)?;
    log::info!("[touch] creating uinput device…");
    let device = create_touchpad_device()?;
    if let Ok(name) = device.sysname() {
        log::info!("[touch] uinput device created: /sys/devices/virtual/input/{}", name.to_string_lossy());
    }
    log::info!("[touch] relative touchpad (finger = cursor move, no absolute coords)");

    let btn_touch_code = Key::BTN_TOUCH.raw();

    let mut buf = [0u8; INPUT_EVENT_SIZE];
    let mut touch_down = false;
    let mut last_x: Option<i32> = None;
    let mut last_y: Option<i32> = None;
    let mut count: u64 = 0;

    let mut frame_contact_count = 0i32;
    let mut frame_slot_active = [false; 16];
    let mut slot_active = frame_slot_active;
    let mut contact_count = frame_contact_count;
    let mut frame_current_slot: usize = 0;
    let mut frame_x: Option<i32> = None;
    let mut frame_y: Option<i32> = None;

    log::info!("[touch] waiting for events (touch the reMarkable screen)…");

    loop {
        channel.read_exact(&mut buf)?;
        if let Some(ev) = parse_input_event(&buf) {
            let ty = ev.event_type().raw();
            let code = ev.raw_code();
            let value = ev.raw_value();

            if ty == EV_KEY {
                continue;
            }
            if ty == EV_ABS {
                if code == ABS_MT_SLOT {
                    frame_current_slot = value.max(0) as usize;
                    if frame_current_slot >= 16 {
                        frame_current_slot = 15;
                    }
                } else if code == ABS_MT_TRACKING_ID {
                    if value >= 0 {
                        if !frame_slot_active[frame_current_slot] {
                            frame_contact_count += 1;
                        }
                        frame_slot_active[frame_current_slot] = true;
                    } else {
                        if frame_slot_active[frame_current_slot] {
                            frame_contact_count = frame_contact_count.saturating_sub(1);
                        }
                        frame_slot_active[frame_current_slot] = false;
                    }
                } else if code == ABS_MT_POSITION_X {
                    frame_x = Some(value);
                } else if code == ABS_MT_POSITION_Y {
                    frame_y = Some(value);
                }
            }

            if ty == EV_SYN && code == SYN_REPORT {
                contact_count = frame_contact_count;
                slot_active = frame_slot_active;

                let mut out: Vec<InputEvent> = Vec::with_capacity(16);

                if contact_count > 0 && !touch_down {
                    out.push(key_event(btn_touch_code, 1));
                    touch_down = true;
                } else if contact_count == 0 && touch_down {
                    out.push(key_event(btn_touch_code, 0));
                    touch_down = false;
                    last_x = None;
                    last_y = None;
                }

                if contact_count > 0 {
                    if let (Some(x), Some(y)) = (frame_x, frame_y) {
                        if let (Some(px), Some(py)) = (last_x, last_y) {
                            out.push(rel_event(REL_X, x - px));
                            out.push(rel_event(REL_Y, y - py));
                        }
                        last_x = Some(x);
                        last_y = Some(y);
                    }
                }

                if !out.is_empty() {
                    out.push(evdevil::event::SynEvent::new(evdevil::event::Syn::REPORT).into());
                    device.write(&out)?;
                    if count == 0 {
                        log::info!("[touch] first event batch (events are flowing)");
                    }
                }

                frame_x = None;
                frame_y = None;
                frame_contact_count = contact_count;
                frame_slot_active = slot_active;
                count += 1;
                log::debug!(
                    "[touch] frame #{} contacts={} out_len={}",
                    count,
                    contact_count,
                    out.len()
                );
                if count % 500 == 0 {
                    log::debug!("[touch] batches: {}", count);
                }
            }
        }
    }
}
