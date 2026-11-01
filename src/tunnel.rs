//! SSH tunnels for PostgreSQL connections.
//!
//! The tunnel opens a port on 127.0.0.1. Each connection to that port goes
//! through the SSH server to the database host and port. The SSH server must
//! be in the known_hosts file of the user, as for the `ssh` command.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::PrivateKeyWithHashAlg;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

/// How the tunnel signs in to the SSH server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SshAuth {
    /// The keys in the running SSH agent.
    #[default]
    Agent,
    /// A private key file, with an optional passphrase.
    KeyFile,
    Password,
}

impl SshAuth {
    pub const ALL: [SshAuth; 3] = [SshAuth::Agent, SshAuth::KeyFile, SshAuth::Password];

    pub fn label(self) -> &'static str {
        match self {
            SshAuth::Agent => "SSH agent",
            SshAuth::KeyFile => "Key file",
            SshAuth::Password => "Password",
        }
    }
}

/// The SSH part of a connection.
pub struct Ssh<'a> {
    pub host: &'a str,
    pub port: u16,
    pub user: &'a str,
    pub auth: SshAuth,
    pub key_file: &'a str,
    /// The password, or the passphrase of the key file.
    pub secret: &'a str,
    pub known_hosts: &'a Path,
}

/// An open tunnel. It closes when it is dropped.
pub struct Tunnel {
    /// The local port that leads to the database.
    pub port: u16,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl std::fmt::Debug for Tunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tunnel(127.0.0.1:{})", self.port)
    }
}

/// The known_hosts file of the user.
pub fn known_hosts() -> Option<PathBuf> {
    Some(dirs_next::home_dir()?.join(".ssh").join("known_hosts"))
}

struct Client {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    /// Why the host key was refused, for the error message.
    refused: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key,
            russh::keys::PublicKeyOrCertificate::Certificate(_) => {
                *self.refused.lock().unwrap() = Some(
                    "The SSH server uses a host certificate. sqlsea checks only host keys \
                     in known_hosts."
                        .into(),
                );
                return Ok(false);
            }
        };
        let reason = match russh::keys::check_known_hosts_path(
            &self.host,
            self.port,
            key,
            &self.known_hosts,
        ) {
            Ok(true) => return Ok(true),
            Ok(false) => format!(
                "The SSH server {} is not in your known_hosts file. \
                 Connect to it once with the ssh command, and make sure that its key is correct.",
                self.host
            ),
            Err(russh::keys::Error::KeyChanged { line }) => format!(
                "The key of the SSH server {} is different from line {line} of your known_hosts \
                 file. The server can be an impostor. Do not connect until you know why the key changed.",
                self.host
            ),
            Err(e) => format!("The known_hosts file did not open: {e}"),
        };
        *self.refused.lock().unwrap() = Some(reason);
        Ok(false)
    }
}

/// Opens a tunnel through `ssh` to `target_host` and `target_port`.
pub async fn open(ssh: Ssh<'_>, target_host: String, target_port: u16) -> Result<Tunnel, String> {
    let refused = Arc::new(Mutex::new(None));
    let handler = Client {
        host: ssh.host.to_string(),
        port: ssh.port,
        known_hosts: ssh.known_hosts.to_path_buf(),
        refused: refused.clone(),
    };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        nodelay: true,
        ..Default::default()
    });
    let connecting = client::connect(config, (ssh.host, ssh.port), handler);
    let mut session = match tokio::time::timeout(Duration::from_secs(10), connecting).await {
        Ok(Ok(session)) => session,
        Ok(Err(e)) => {
            return Err(match refused.lock().unwrap().take() {
                Some(reason) => reason,
                None => format!("The SSH connection to {} failed: {e}", ssh.host),
            });
        }
        Err(_) => {
            return Err(format!(
                "The SSH server {} did not answer in time.",
                ssh.host
            ));
        }
    };

    if !authenticate(&mut session, &ssh).await? {
        return Err(format!(
            "The SSH server did not accept the sign-in of {}. Make sure that the user and the {} are correct.",
            ssh.user,
            match ssh.auth {
                SshAuth::Agent => "keys in the SSH agent",
                SshAuth::KeyFile => "key file",
                SshAuth::Password => "password",
            }
        ));
    }

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("The local port for the tunnel did not open: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let session = Arc::new(session);
    let task = tokio::spawn(async move {
        while let Ok((mut socket, from)) = listener.accept().await {
            let session = session.clone();
            let host = target_host.clone();
            tokio::spawn(async move {
                let channel = session
                    .channel_open_direct_tcpip(
                        host,
                        target_port.into(),
                        "127.0.0.1",
                        from.port().into(),
                    )
                    .await;
                if let Ok(channel) = channel {
                    let mut stream = channel.into_stream();
                    let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                }
            });
        }
    });
    Ok(Tunnel { port, task })
}

/// Signs in with the chosen method. Returns false if the server refuses.
async fn authenticate(session: &mut client::Handle<Client>, ssh: &Ssh<'_>) -> Result<bool, String> {
    let user = ssh.user.to_string();
    match ssh.auth {
        SshAuth::Password => session
            .authenticate_password(user, ssh.secret)
            .await
            .map(|r| r.success())
            .map_err(|e| e.to_string()),
        SshAuth::KeyFile => {
            let path = ssh.key_file.trim();
            if path.is_empty() {
                return Err("Choose the private key file for SSH.".into());
            }
            let passphrase = (!ssh.secret.is_empty()).then_some(ssh.secret);
            let key = russh::keys::load_secret_key(path, passphrase).map_err(|e| {
                format!(
                    "The SSH key file did not open. If the key has a passphrase, \
                     make sure that it is correct.\n{e}"
                )
            })?;
            let hash = session
                .best_supported_rsa_hash()
                .await
                .map_err(|e| e.to_string())?
                .flatten();
            session
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map(|r| r.success())
                .map_err(|e| e.to_string())
        }
        SshAuth::Agent => agent(session, user).await,
    }
}

#[cfg(unix)]
async fn agent(session: &mut client::Handle<Client>, user: String) -> Result<bool, String> {
    let mut agent = russh::keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|_| {
            "No SSH agent is running. Start one and add your key with ssh-add, \
             or choose Key file."
                .to_string()
        })?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| e.to_string())?;
    if identities.is_empty() {
        return Err("The SSH agent has no keys. Add your key with ssh-add.".into());
    }
    let hash = session
        .best_supported_rsa_hash()
        .await
        .map_err(|e| e.to_string())?
        .flatten();
    for identity in identities {
        let key = identity.public_key().into_owned();
        let result = session
            .authenticate_publickey_with(user.clone(), key, hash, &mut agent)
            .await
            .map_err(|e| e.to_string())?;
        if result.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(not(unix))]
async fn agent(_: &mut client::Handle<Client>, _: String) -> Result<bool, String> {
    Err("The SSH agent works on Linux and macOS. On Windows, choose Key file or Password.".into())
}
