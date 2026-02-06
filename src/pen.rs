//! Forward reMarkable pen input to a uinput pen device.

use std::io::Read;
use std::path::Path;

use evdevil::event::{Abs, Key};
use evdevil::uinput::{AbsSetup, UinputDevice};
use evdevil::AbsInfo;
use evdevil::Bus;
use evdevil::InputId;
use evdevil::InputProp;

use crate::config::PEN_DEVICE;
use crate::event::{parse_input_event, INPUT_EVENT_SIZE};
use crate::ssh;

fn create_pen_device() -> Result<UinputDevice, Box<dyn std::error::Error + Send + Sync>> {
    let axes = [
        AbsSetup::new(Abs::X, AbsInfo::new(0, 4095)),
        AbsSetup::new(Abs::Y, AbsInfo::new(0, 4095)),
        AbsSetup::new(Abs::PRESSURE, AbsInfo::new(0, 4095)),
        AbsSetup::new(Abs::TILT_X, AbsInfo::new(-30, 30)),
        AbsSetup::new(Abs::TILT_Y, AbsInfo::new(-30, 30)),
    ];
    // INPUT_PROP_DIRECT = tablet (direct input). InputId so KDE/tablet settings recognize it.
    let device = UinputDevice::builder()?
        .with_input_id(InputId::new(Bus::from_raw(0x06), 0x2d1f, 0x0001, 0))? // BUS_VIRTUAL, reMarkable-ish vendor
        .with_props([InputProp::DIRECT])?
        .with_abs_axes(axes)?
        .with_keys([Key::BTN_TOOL_PEN, Key::BTN_TOUCH, Key::BTN_STYLUS])?
        .build("reMarkable Pen")?;
    Ok(device)
}

pub fn run(key_path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (_sess, mut channel) = ssh::open_input_stream(PEN_DEVICE, key_path)?;
    log::info!("[pen] creating uinput device…");
    let device = create_pen_device()?;
    if let Ok(name) = device.sysname() {
        log::info!("[pen] uinput device created: /sys/devices/virtual/input/{}", name.to_string_lossy());
    }
    log::info!("[pen] forwarding (move pen on tablet to see events)");

    let mut buf = [0u8; INPUT_EVENT_SIZE];
    let mut count: u64 = 0;
    loop {
        channel.read_exact(&mut buf)?;
        if let Some(ev) = parse_input_event(&buf) {
            device.write(&[ev])?;
            count += 1;
            if count == 1 {
                log::info!("[pen] first event received (events are flowing)");
            } else if count % 500 == 0 {
                log::debug!("[pen] events forwarded: {}", count);
            }
        }
    }
}
