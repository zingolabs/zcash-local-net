//! Module for the structs that represent and manage the indexer processes i.e. Zainod.
//!
//! Processes which are not strictly indexers but have a similar role in serving light-clients/light-wallets
//! (i.e. Lightwalletd) are also included in this category and are referred to as "light-nodes".

use std::{fs::File, path::PathBuf, process::Child};

use getset::{CopyGetters, Getters};
use portpicker::Port;
use tempfile::TempDir;

use crate::{config, error::LaunchError, launch, logs, network, Process};

/// Enumeration of all config structs associated with indexer/light-node processes
pub enum IndexerConfig {}

/// Zainod configuration
///
/// Use `fixed_port` to specify a port for Zainod. Otherwise, a port is picked at random between 15000-25000.
///
/// The `validator_port` must be specified and the validator process must be running before launching Zainod.
pub struct ZainodConfig {
    /// Zainod binary location
    pub zainod_bin: Option<PathBuf>,
    /// Listen RPC port
    pub listen_port: Option<Port>,
    /// Validator RPC port
    pub validator_port: Port,
}

/// Lightwalletd configuration
///
/// Use `fixed_port` to specify a port for Lightwalletd. Otherwise, a port is picked at random between 15000-25000.
///
/// The `validator_port` must be specified and the validator process must be running before launching Lightwalletd.
pub struct LightwalletdConfig {
    /// Lightwalletd binary location
    pub lightwalletd_bin: Option<PathBuf>,
    /// Listen RPC port
    pub listen_port: Option<Port>,
    /// Validator configuration file location
    pub validator_conf: PathBuf,
}

/// Functionality for indexer/light-node processes.
pub trait Indexer {
    /// Config filename
    const CONFIG_FILENAME: &str;

    /// Stops the process.
    fn stop(&mut self);

    /// Get temporary config directory.
    fn config_dir(&self) -> &TempDir;

    /// Get temporary logs directory.
    fn logs_dir(&self) -> &TempDir;

    /// Returns path to config file.
    fn config_path(&self) -> PathBuf {
        self.config_dir().path().join(Self::CONFIG_FILENAME)
    }

    /// Prints the stdout log.
    fn print_stdout(&self) {
        let stdout_log_path = self.logs_dir().path().join(logs::STDOUT_LOG);
        logs::print_log(stdout_log_path);
    }

    /// Prints the stdout log.
    fn print_stderr(&self) {
        let stdout_log_path = self.logs_dir().path().join(logs::STDERR_LOG);
        logs::print_log(stdout_log_path);
    }
}

/// This struct is used to represent and manage the Zainod process.
#[derive(Getters, CopyGetters)]
#[getset(get = "pub")]
pub struct Zainod {
    /// Child process handle
    handle: Child,
    /// RPC port
    #[getset(skip)]
    #[getset(get_copy = "pub")]
    port: Port,
    /// Logs directory
    logs_dir: TempDir,
    /// Config directory
    config_dir: TempDir,
}

impl Zainod {
    /// Launches Zainod process and returns [`crate::Zainod`] with the handle and associated directories.
    pub fn launch(config: ZainodConfig) -> Result<Zainod, LaunchError> {
        let logs_dir = tempfile::tempdir().unwrap();

        let port = network::pick_unused_port(config.listen_port);
        let config_dir = tempfile::tempdir().unwrap();
        let config_file_path =
            config::zainod(config_dir.path(), port, config.validator_port).unwrap();

        let mut command = match config.zainod_bin {
            Some(path) => std::process::Command::new(path),
            None => std::process::Command::new("zainod"),
        };
        command
            .args([
                "--config",
                config_file_path.to_str().expect("should be valid UTF-8"),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut handle = command.spawn().unwrap();

        logs::write_logs(&mut handle, &logs_dir);
        launch::wait(
            Process::Zainod,
            &mut handle,
            &logs_dir,
            None,
            "Server Ready.",
            "Error:",
        )?;

        Ok(Zainod {
            handle,
            port,
            logs_dir,
            config_dir,
        })
    }
}

impl Indexer for Zainod {
    const CONFIG_FILENAME: &str = config::ZAINOD_FILENAME;

    fn stop(&mut self) {
        self.handle.kill().expect("zainod couldn't be killed")
    }

    fn config_dir(&self) -> &TempDir {
        &self.config_dir
    }

    fn logs_dir(&self) -> &TempDir {
        &self.logs_dir
    }
}

impl Drop for Zainod {
    fn drop(&mut self) {
        self.stop();
    }
}

/// This struct is used to represent and manage the Lightwalletd process.
#[derive(Getters, CopyGetters)]
#[getset(get = "pub")]
pub struct Lightwalletd {
    /// Child process handle
    handle: Child,
    /// RPC Port
    #[getset(skip)]
    #[getset(get_copy = "pub")]
    port: Port,
    /// Data directory
    _data_dir: TempDir,
    /// Logs directory
    logs_dir: TempDir,
    /// Config directory
    config_dir: TempDir,
}

impl Lightwalletd {
    /// Launches Lightwalletd process and returns [`crate::Lightwalletd`] with the handle and associated directories.
    pub fn launch(config: LightwalletdConfig) -> Result<Lightwalletd, LaunchError> {
        let logs_dir = tempfile::tempdir().unwrap();
        let lwd_log_file_path = logs_dir.path().join(logs::LIGHTWALLETD_LOG);
        let _lwd_log_file = File::create(&lwd_log_file_path).unwrap();

        let data_dir = tempfile::tempdir().unwrap();

        let port = network::pick_unused_port(config.listen_port);
        let config_dir = tempfile::tempdir().unwrap();
        let config_file_path = config::lightwalletd(
            config_dir.path(),
            port,
            lwd_log_file_path.clone(),
            config.validator_conf.clone(),
        )
        .unwrap();

        let mut command = match config.lightwalletd_bin {
            Some(path) => std::process::Command::new(path),
            None => std::process::Command::new("lightwalletd"),
        };
        command
            .args([
                "--no-tls-very-insecure",
                "--data-dir",
                data_dir.path().to_str().unwrap(),
                "--log-file",
                lwd_log_file_path.to_str().unwrap(),
                "--zcash-conf-path",
                config.validator_conf.to_str().unwrap(),
                "--config",
                config_file_path.to_str().unwrap(),
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut handle = command.spawn().unwrap();

        logs::write_logs(&mut handle, &logs_dir);
        launch::wait(
            Process::Lightwalletd,
            &mut handle,
            &logs_dir,
            Some(lwd_log_file_path),
            "Starting insecure no-TLS (plaintext) server",
            "Error:",
        )?;

        Ok(Lightwalletd {
            handle,
            port,
            _data_dir: data_dir,
            logs_dir,
            config_dir,
        })
    }

    /// Prints the stdout log.
    pub fn print_lwd_log(&self) {
        let stdout_log_path = self.logs_dir.path().join(logs::LIGHTWALLETD_LOG);
        logs::print_log(stdout_log_path);
    }
}

impl Indexer for Lightwalletd {
    const CONFIG_FILENAME: &str = config::LIGHTWALLETD_FILENAME;

    fn stop(&mut self) {
        self.handle.kill().expect("zainod couldn't be killed")
    }

    fn config_dir(&self) -> &TempDir {
        &self.config_dir
    }

    fn logs_dir(&self) -> &TempDir {
        &self.logs_dir
    }
}

impl Drop for Lightwalletd {
    fn drop(&mut self) {
        self.stop();
    }
}
