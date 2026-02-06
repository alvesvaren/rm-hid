use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ssh2::Session;

use crate::config::{Auth, Config};

const SSH_USER: &str = "root";
const SSH_PORT: u16 = 22;

const XOCHITL_STOP_LOOP: &str = "trap 'kill $STOPPER 2>/dev/null; kill -CONT $(pidof xochitl) 2>/dev/null' EXIT; (while true; do kill -STOP $(pidof xochitl) 2>/dev/null; sleep 1; done) & STOPPER=$!; sleep 0.5; cat {}";
const XOCHITL_RESUME: &str = "kill -CONT $(pidof xochitl) 2>/dev/null";

/// Guard that resumes xochitl when the last stream using stop_ui mode is dropped.
pub struct XochitlPauseGuard {
    config: Config,
    refcount: Arc<AtomicUsize>,
}

impl XochitlPauseGuard {
    fn new(config: Config, refcount: Arc<AtomicUsize>) -> Self {
        Self { config, refcount }
    }
}

impl Drop for XochitlPauseGuard {
    fn drop(&mut self) {
        if self.refcount.fetch_sub(1, Ordering::SeqCst) != 1 {
            return;
        }

        if let Err(e) = resume_xochitl(&self.config) {
            log::warn!("Failed to resume xochitl: {}", e);
        }
    }
}

/// Open an SSH connection and stream input from a device.
pub fn open_input_stream(
    device_path: &str,
    config: &Config,
    stop_ui: bool,
    pause_refcount: Option<Arc<AtomicUsize>>,
) -> Result<(Session, ssh2::Channel, Option<XochitlPauseGuard>), Box<dyn std::error::Error + Send + Sync>>
{
    log::info!("Connecting to {}", config.host);

    let session = connect_and_authenticate(config)?;
    let mut channel = session.channel_session()?;

    let cmd = build_stream_command(device_path, stop_ui);
    log::debug!("Executing: {}", cmd);

    channel.exec(&cmd)?;
    channel.handle_extended_data(ssh2::ExtendedData::Merge)?;

    let guard = if stop_ui {
        let refcount = pause_refcount.unwrap_or_else(|| Arc::new(AtomicUsize::new(0)));
        refcount.fetch_add(1, Ordering::SeqCst);
        Some(XochitlPauseGuard::new(config.clone(), refcount))
    } else {
        None
    };

    log::info!("Stream ready for {}", device_path);
    Ok((session, channel, guard))
}

/// Run a single command on the reMarkable.
pub fn run_command(
    config: &Config,
    command: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session = connect_and_authenticate(config)?;
    let mut channel = session.channel_session()?;

    channel.exec(command)?;
    channel.wait_close()?;

    let status = channel.exit_status().unwrap_or(-1);
    if status != 0 {
        return Err(format!("Command exited with status {}", status).into());
    }

    Ok(())
}

fn connect_and_authenticate(config: &Config) -> Result<Session, Box<dyn std::error::Error + Send + Sync>> {
    let tcp = TcpStream::connect((config.host.as_str(), SSH_PORT))?;
    let mut session = Session::new()?;

    session.set_tcp_stream(tcp);
    session.handshake()?;

    authenticate(&mut session, &config.auth())?;

    Ok(session)
}

fn authenticate(session: &mut Session, auth: &Auth) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

fn build_stream_command(device_path: &str, stop_ui: bool) -> String {
    if stop_ui {
        log::info!("Using stop-ui mode (xochitl will resume on disconnect)");
        XOCHITL_STOP_LOOP.replace("{}", device_path)
    } else {
        format!("cat {}", device_path)
    }
}

fn resume_xochitl(config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Resuming xochitl");
    run_command(config, &format!("sh -c '{}'", XOCHITL_RESUME))
}
