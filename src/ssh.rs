//! SSH connection to reMarkable and streaming from remote /dev/input.

use std::io::Read;
use std::net::TcpStream;
use std::path::Path;

use ssh2::Session;

use crate::config::{HOST, USER};

/// Connect to the reMarkable and run `cat <device_path>`, returning the channel to read from.
/// The session must be kept alive while reading.
pub fn open_input_stream(
    device_path: &str,
    key_path: &Path,
) -> Result<(Session, impl Read + Send), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("SSH connecting to {}…", HOST);
    let tcp = TcpStream::connect((HOST, 22))?;
    let mut sess = Session::new()?;
    sess.set_tcp_stream(tcp);
    sess.handshake()?;
    sess.userauth_pubkey_file(USER, None, key_path, None)?;
    if !sess.authenticated() {
        return Err("SSH auth failed".into());
    }
    log::info!("SSH connected, running cat {}…", device_path);
    let mut channel = sess.channel_session()?;
    channel.exec(&format!("cat {}", device_path))?;
    channel.handle_extended_data(ssh2::ExtendedData::Merge)?;
    log::info!("stream ready for {}", device_path);
    Ok((sess, channel))
}
