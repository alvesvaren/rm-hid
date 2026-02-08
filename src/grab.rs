//! Upload and manage the evgrab helper binary on the reMarkable.
//!
//! The evgrab helper is a tiny static ARM binary that exclusively grabs an
//! evdev device via EVIOCGRAB and pipes events to stdout. This prevents
//! xochitl from seeing input without stopping the process (which would
//! trigger the watchdog).
//!
//! Binaries for both armv7 (rM2) and aarch64 (rMPP/rMPM) are embedded at
//! compile time and the correct one is uploaded over SSH on first connect.

use std::fmt;
use std::io::{Read, Write};

use ssh2::Session;

const GRAB_ARMV7: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/evgrab-armv7"));
const GRAB_AARCH64: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/evgrab-aarch64"));

const REMOTE_PATH: &str = "/tmp/rm-mouse-grab";

#[derive(Debug, Clone, Copy)]
pub enum Arch {
    Armv7,
    Aarch64,
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arch::Armv7 => write!(f, "armv7"),
            Arch::Aarch64 => write!(f, "aarch64"),
        }
    }
}

/// Detect the tablet's CPU architecture by running `uname -m` over SSH.
pub fn detect_arch(session: &Session) -> Result<Arch, Box<dyn std::error::Error + Send + Sync>> {
    let mut channel = session.channel_session()?;
    channel.exec("uname -m")?;

    let mut output = String::new();
    channel.read_to_string(&mut output)?;

    // Explicitly close our end so the session is left in a clean state
    // for subsequent channels (SFTP, exec, etc.).
    channel.close()?;
    channel.wait_close()?;

    match output.trim() {
        "armv7l" => Ok(Arch::Armv7),
        "aarch64" => Ok(Arch::Aarch64),
        other => Err(format!("Unsupported tablet architecture: {}", other).into()),
    }
}

/// Upload the correct grab helper binary to the tablet.
///
/// Pipes the binary through `cat` into a file on the tablet and marks it
/// executable. This avoids the SFTP subsystem which can hang on some
/// SSH implementations.
pub fn upload_helper(
    session: &Session,
    arch: Arch,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let binary = match arch {
        Arch::Armv7 => GRAB_ARMV7,
        Arch::Aarch64 => GRAB_AARCH64,
    };

    log::info!(
        "Uploading grab helper ({}, {} bytes) to {}",
        arch,
        binary.len(),
        REMOTE_PATH
    );

    let mut channel = session.channel_session()?;
    // Write to a PID-unique temp file and atomically rename into place.
    // This avoids corruption when pen and touch threads upload concurrently.
    channel.exec(&format!(
        "cat > {path}.$$ && chmod +x {path}.$$ && mv -f {path}.$$ {path}",
        path = REMOTE_PATH
    ))?;

    channel.write_all(binary)?;
    channel.send_eof()?;
    channel.wait_eof()?;
    channel.close()?;
    channel.wait_close()?;

    let status = channel.exit_status()?;
    if status != 0 {
        return Err(format!("Failed to upload grab helper (exit status {})", status).into());
    }

    log::info!("Grab helper uploaded successfully");
    Ok(())
}

/// Build the remote command that grabs a device and streams events.
///
/// Stderr is redirected to a log file on the tablet for diagnostics.
/// Uses `exec` to replace the shell with the grab helper so that signal
/// delivery (on SSH disconnect) goes directly to the right process.
pub fn grab_command(device_path: &str) -> String {
    format!(
        "exec {} {} 2>>{}.log",
        REMOTE_PATH, device_path, REMOTE_PATH
    )
}
