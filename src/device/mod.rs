mod rm2;

pub use rm2::RM2;

/// Device-specific parameters for input handling.
#[derive(Debug, Clone, Copy)]
pub struct DeviceProfile {
    #[allow(dead_code)]
    pub name: &'static str,

    // Pen digitizer ranges
    pub pen_x_min: i32,
    pub pen_x_max: i32,
    pub pen_y_min: i32,
    pub pen_y_max: i32,
    pub pen_pressure_max: i32,
    pub pen_distance_max: i32,
    pub pen_tilt_range: i32,

    // Touch screen dimensions
    pub touch_x_max: i32,
    pub touch_y_max: i32,
    pub touch_resolution: i32,

    // Default device paths
    pub pen_device: &'static str,
    pub touch_device: &'static str,
}

impl DeviceProfile {
    /// Get profile for the current device (defaults to RM2).
    pub fn current() -> &'static Self {
        &RM2
    }
}
