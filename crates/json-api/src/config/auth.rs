//! Auth Config

use clap::Args;

/// `OpenBao` authentication settings.
#[derive(Debug, Args)]
pub struct AuthConfig {
    /// `OpenBao` server address
    #[arg(long, env = "OPENBAO_ADDR")]
    pub addr: String,

    /// `OpenBao` authentication token
    #[arg(long, env = "OPENBAO_TOKEN", hide_env_values = true)]
    pub token: String,

    /// `OpenBao` Transit key name
    #[arg(long, env = "OPENBAO_TRANSIT_KEY")]
    pub transit_key: String,
}
