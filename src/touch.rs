//! Forward reMarkable touch input to a uinput touchpad.

use std::io::Read;
use std::path::Path;

use evdevil::event::Abs;
use evdevil::uinput::{AbsSetup, UinputDevice};
use evdevil::AbsInfo;
use evdevil::InputProp;

use crate::config::TOUCH_DEVICE;
use crate::event::{parse_input_event, INPUT_EVENT_SIZE};
use crate::ssh;

fn create_touchpad_device() -> Result<UinputDevice, Box<dyn std::error::Error + Send + Sync>> {
    let axes = [
        AbsSetup::new(Abs::X, AbsInfo::new(0, 4095)),
        AbsSetup::new(Abs::Y, AbsInfo::new(0, 4095)),
        AbsSetup::new(Abs::MT_POSITION_X, AbsInfo::new(0, 4095)),
        AbsSetup::new(Abs::MT_POSITION_Y, AbsInfo::new(0, 4095)),
        AbsSetup::new(Abs::MT_SLOT, AbsInfo::new(0, 9)),
        AbsSetup::new(Abs::MT_TRACKING_ID, AbsInfo::new(-1, 65535)),
    ];
    // INPUT_PROP_POINTER = treat as touchpad (cursor visible), not direct touchscreen.
    let device = UinputDevice::builder()?
        .with_props([InputProp::POINTER])?
        .with_abs_axes(axes)?
        .with_keys([evdevil::event::Key::BTN_LEFT, evdevil::event::Key::BTN_TOUCH])?
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
    log::info!("[touch] forwarding (touch screen to see events)");

    let mut buf = [0u8; INPUT_EVENT_SIZE];
    let mut count: u64 = 0;
    loop {
        channel.read_exact(&mut buf)?;
        if let Some(ev) = parse_input_event(&buf) {
            device.write(&[ev])?;
            count += 1;
            if count == 1 {
                log::info!("[touch] first event received (events are flowing)");
            } else if count % 500 == 0 {
                log::debug!("[touch] events forwarded: {}", count);
            }
        }
    }
}
