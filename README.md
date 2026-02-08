# rm-pad

This is a simple program that takes input from a reMarkable tablet (only for rm2 right now) and converts it into libinput devices. This project only works on linux, and is only tested on wayland.

Features:
- Pen input (position, pressure and tilt)
- Touch input (multi-touch gestures, tapping and moving)
- Configurable palm rejection (disables touch input for a configurable grace period if any pen input is detected, default 500ms)
- Screen orientation support (portrait, landscape-right, landscape-left, inverted)
- Optional input grab so the tablet UI doesn't see input: with `--stop-ui` or `stop_ui = true` in config, a small helper binary is uploaded to `/tmp` on the tablet and uses `EVIOCGRAB` to exclusively grab the input devices. The tablet UI (xochitl) keeps running but receives no pen/touch events. The grab is automatically released when rm-pad exits or the SSH connection drops — no reboot or manual cleanup needed.
- Works over both wifi and USB
- Very low latency (as long as your connection to the tablet is fast)
- Runs in userspace (as long as your user is allowed to create input devices)
- Debug mode: `rm-pad dump touch` or `rm-pad dump pen` to dump raw input events

## Installation

Either build it yourself or use the prebuilt binaries from GitHub releases.

### Building from source

You'll need Rust and C cross-compilers for ARM:

**Ubuntu/Debian:**
```bash
sudo apt install gcc-arm-linux-gnueabihf gcc-aarch64-linux-gnu
```

**Arch Linux:**
```bash
sudo pacman -S arm-linux-gnueabihf-gcc aarch64-linux-gnu-gcc
```

You can also set `ARMV7_CC` and `AARCH64_CC` environment variables to point to your cross-compilers.

Then build with:
```bash
cargo build --release
```

### Setup (required for userspace operation)

To allow rm-pad to create virtual input devices, you need to set up udev rules:

```bash
sudo cp data/50-uinput.rules /etc/udev/rules.d/
sudo groupadd -f uinput
sudo usermod -aG uinput $USER
```

Then log out and back in (or reboot), and reload udev rules:
```bash
sudo udevadm control --reload-rules
```

## Configuration

Config file search order:
1. `RMPAD_CONFIG` environment variable (if set)
2. `./rm-pad.toml` (current directory)
3. `~/.config/rm-pad/config.toml` (user config directory)

Copy the `rm-pad.toml.example` file to one of these locations (recommended: `~/.config/rm-pad/config.toml`) and change the options to your preferences.

### Connection settings

- **host**: reMarkable tablet IP address or hostname. Default is `10.11.99.1` (USB connection). For WiFi, use your tablet's IP address.
- **key_path**: Path to SSH private key for authentication (default: `"rm-key"`). Only used if `password` is not set.
- **password**: Root password for SSH authentication. If set, `key_path` is ignored. **Warning**: Restrict file permissions with `chmod 600` if storing password in config file.

You can also use environment variables:
- `RMPAD_HOST`: Override host
- `RMPAD_PASSWORD`: Override password
- `RMPAD_CONFIG`: Override config file path

### Behavior options

- **touch_only**: Run touch input only (no pen)
- **pen_only**: Run pen input only (no touch)
- **stop_ui**: Stop xochitl UI while streaming input (uses input grab)
- **no_palm_rejection**: Disable palm rejection
- **palm_grace_ms**: Palm rejection grace period in milliseconds (default: 500)
- **orientation**: Screen orientation - `portrait`, `landscape-right` (default), `landscape-left`, or `inverted`

All options can also be set via command-line flags. Run `rm-pad --help` for details.

## Usage

Run `rm-pad` to start forwarding input. The program will automatically reconnect if the connection drops.

For debugging, use the dump command:
```bash
rm-pad dump touch  # Dump raw touch events
rm-pad dump pen    # Dump raw pen events
```
