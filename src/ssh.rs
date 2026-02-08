use std::net::TcpStream;

use ssh2::Session;

use crate::config::{Auth, Config};
use crate::grab;

const SSH_USER: &str = "root";
const SSH_PORT: u16 = 22;

/// Open an SSH connection and stream input from a device.
///
/// When `grab` is true, the evgrab helper is uploaded to the tablet and
/// used to exclusively grab the device (EVIOCGRAB). This prevents xochitl
/// from seeing input without stopping the process. The grab is automatically
/// released when the SSH channel closes (disconnect, signal, etc.).
///
/// When `grab` is false, plain `cat` is used and xochitl also sees events.
pub fn open_input_stream(
    device_path: &str,
    config: &Config,
    grab: bool,
) -> Result<(Session, ssh2::Channel), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Connecting to {}", config.host);

    let session = connect_and_authenticate(config)?;

    if grab {
        prepare_grab(&session)?;
    }

    let mut channel = session.channel_session()?;

    let cmd = build_stream_command(device_path, grab);
    log::debug!("Executing: {}", cmd);

    channel.exec(&cmd)?;
    channel.handle_extended_data(ssh2::ExtendedData::Merge)?;

    log::info!("Stream ready for {}", device_path);
    Ok((session, channel))
}

fn connect_and_authenticate(
    config: &Config,
) -> Result<Session, Box<dyn std::error::Error + Send + Sync>> {
    let tcp = TcpStream::connect((config.host.as_str(), SSH_PORT))?;
    let mut session = Session::new()?;

    session.set_tcp_stream(tcp);
    session.handshake()?;

    authenticate(&mut session, &config.auth())?;

    Ok(session)
}

fn authenticate(
    session: &mut Session,
    auth: &Auth,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match auth {
        Auth::Key(path) => {
            session.userauth_pubkey_file(SSH_USER, None, path.as_ref(), None)?;
        }
        Auth::Password(pass) => {
            session.userauth_password(SSH_USER, pass)?;
        }
    }

    if !session.authenticated() {
        return Err("SSH authentication failed".into());
    }

    Ok(())
}

/// Detect the tablet architecture and upload the grab helper via SFTP.
fn prepare_grab(session: &Session) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let arch = grab::detect_arch(session)?;
    log::info!("Detected tablet architecture: {}", arch);

    grab::upload_helper(session, arch)?;
    Ok(())
}

fn build_stream_command(device_path: &str, grab: bool) -> String {
    if grab {
        log::info!("Using grab mode (input restored automatically on disconnect)");
        grab::grab_command(device_path)
    } else {
        format!("cat {}", device_path)
    }
}
