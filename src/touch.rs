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
    ABS_MT_TRACKING_ID, EV_ABS, EV_KEY, EV_SYN, INPUT_EVENT_SIZE, REL_HWHEEL, REL_WHEEL, REL_X,
    REL_Y, SYN_REPORT,
};
use crate::ssh;

fn create_touchpad_device() -> Result<UinputDevice, Box<dyn std::error::Error + Send + Sync>> {
    // Kernel uinput docs: virtual mouse must declare BTN_LEFT so REL_X/REL_Y move the cursor.
    // We only emit BTN_TOUCH for touch down/up; BTN_LEFT is never sent (avoids stuck-button).
    // REL_WHEEL/REL_HWHEEL for two-finger scroll.
    let device = UinputDevice::builder()?
        .with_props([InputProp::POINTER])?
        .with_rel_axes([Rel::X, Rel::Y, Rel::WHEEL, Rel::HWHEEL])?
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
    let mut count: u64 = 0;

    // Per-slot position (updated when we see POSITION_X/Y for that slot; cleared on TRACKING_ID -1).
    let mut slot_x: [Option<i32>; 16] = [None; 16];
    let mut slot_y: [Option<i32>; 16] = [None; 16];
    let mut frame_slot_active = [false; 16];
    #[allow(unused_assignments)]
    let mut slot_active = frame_slot_active;
    let mut primary_slot: Option<usize> = None; // slot we use for cursor (stable until it lifts)
    let mut last_primary_x: Option<i32> = None;
    let mut last_primary_y: Option<i32> = None;
    // Two-finger scroll: centroid of all fingers last frame (for delta).
    let mut last_centroid_x: Option<i64> = None;
    let mut last_centroid_y: Option<i64> = None;

    let mut frame_contact_count = 0i32;
    let mut frame_current_slot: usize = 0;

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
                        slot_x[frame_current_slot] = None;
                        slot_y[frame_current_slot] = None;
                    }
                } else if code == ABS_MT_POSITION_X {
                    slot_x[frame_current_slot] = Some(value);
                } else if code == ABS_MT_POSITION_Y {
                    slot_y[frame_current_slot] = Some(value);
                }
            }

            if ty == EV_SYN && code == SYN_REPORT {
                let contact_count = frame_contact_count;
                slot_active = frame_slot_active;

                // Primary = one finger we use for cursor. Keep same finger until it lifts, then use first active.
                let first_active_slot = || (0..16).find(|&i| slot_active[i]);
                let new_primary = if contact_count == 0 {
                    None
                } else if primary_slot.map_or(false, |s| s < 16 && slot_active[s]) {
                    primary_slot
                } else {
                    first_active_slot()
                };

                if new_primary != primary_slot {
                    primary_slot = new_primary;
                    if let Some(s) = primary_slot {
                        last_primary_x = slot_x[s];
                        last_primary_y = slot_y[s];
                    } else {
                        last_primary_x = None;
                        last_primary_y = None;
                    }
                }

                let mut out: Vec<InputEvent> = Vec::with_capacity(16);

                if contact_count > 0 && !touch_down {
                    out.push(key_event(btn_touch_code, 1));
                    touch_down = true;
                } else if contact_count == 0 && touch_down {
                    out.push(key_event(btn_touch_code, 0));
                    touch_down = false;
                }

                if contact_count >= 2 {
                    // Two-finger scroll: centroid delta -> REL_WHEEL / REL_HWHEEL.
                    let (sum_x, sum_y): (i64, i64) = (0..16)
                        .filter(|&i| slot_active[i])
                        .filter_map(|i| {
                            slot_x[i].zip(slot_y[i]).map(|(a, b)| (a as i64, b as i64))
                        })
                        .fold((0i64, 0i64), |(sx, sy), (x, y)| (sx + x, sy + y));
                    let n = contact_count as i64;
                    if n > 0 {
                        let cx = sum_x / n;
                        let cy = sum_y / n;
                        if let (Some(lx), Some(ly)) = (last_centroid_x, last_centroid_y) {
                            let dx = (cx - lx).clamp(-200, 200);
                            let dy = (cy - ly).clamp(-200, 200);
                            // Finger up = scroll up (positive REL_WHEEL). Scale to ~1–2 ticks per small move.
                            let wheel = (-dy / 30).clamp(-15, 15);
                            let hwheel = (dx / 30).clamp(-15, 15);
                            if wheel != 0 {
                                out.push(rel_event(REL_WHEEL, wheel as i32));
                            }
                            if hwheel != 0 {
                                out.push(rel_event(REL_HWHEEL, hwheel as i32));
                            }
                        }
                        last_centroid_x = Some(cx);
                        last_centroid_y = Some(cy);
                    }
                } else {
                    last_centroid_x = None;
                    last_centroid_y = None;
                    if let Some(s) = primary_slot {
                        if let (Some(x), Some(y)) = (slot_x[s], slot_y[s]) {
                            if let (Some(px), Some(py)) = (last_primary_x, last_primary_y) {
                                out.push(rel_event(REL_X, x - px));
                                out.push(rel_event(REL_Y, y - py));
                            }
                            last_primary_x = Some(x);
                            last_primary_y = Some(y);
                        }
                    }
                }

                if !out.is_empty() {
                    out.push(evdevil::event::SynEvent::new(evdevil::event::Syn::REPORT).into());
                    device.write(&out)?;
                    if count == 0 {
                        log::info!("[touch] first event batch (events are flowing)");
                    }
                }

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
