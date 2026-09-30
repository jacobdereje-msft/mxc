// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

const MANAGED_MOUNTS_BEGIN: &str = "# BEGIN MXC managed mounts (rewritten every run)";
const MANAGED_MOUNTS_END: &str = "# END MXC managed mounts";

const DHCPCD_CONF_NOARP: &str =
    "# MXC: the bridge's DHCP server is authoritative for this subnet and\n\
     # probes each address before it offers it.\n\
     noarp\n";

/// The bridge whose DHCP server probes an address before it offers it.
const PROBING_DHCP_BRIDGE: &str = "lxcbr0";

/// The most of a container's `dhcpcd.conf` the host will read.
///
/// The workload owns this file, and a container kept for reuse carries what
/// the workload left behind into the next run.
const DHCPCD_CONF_MAX_LEN: u64 = 64 * 1024;

/// The filesystem type and options of one kind of `lxc.mount.entry`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MountShape {
    filesystem: &'static str,
    options: &'static str,
}

impl MountShape {
    pub(crate) fn entry(self, source: &str, target: &str) -> String {
        format!(
            "{} {} {} {} 0 0",
            source, target, self.filesystem, self.options
        )
    }
}

pub(crate) const MOUNT_READWRITE: MountShape = MountShape {
    filesystem: "none",
    options: "bind,create=dir",
};
pub(crate) const MOUNT_READONLY: MountShape = MountShape {
    filesystem: "none",
    options: "bind,ro,create=dir",
};
pub(crate) const MASK_FILE: MountShape = MountShape {
    filesystem: "none",
    options: "bind,ro,create=file",
};
pub(crate) const MASK_DIR_HOLDING_MOUNTPOINTS: MountShape = MountShape {
    filesystem: "tmpfs",
    options: "size=1m,create=dir",
};
pub(crate) const MASK_DIR: MountShape = MountShape {
    filesystem: "tmpfs",
    options: "ro,size=0,create=dir",
};

/// Resolve the default LXC storage path the way liblxc does.
fn resolve_lxcpath_with_env<F, G>(get_env: F, geteuid: G) -> String
where
    F: Fn(&str) -> Option<String>,
    G: Fn() -> u32,
{
    if let Some(p) = get_env("LXC_PATH") {
        if !p.is_empty() {
            return p;
        }
    }
    if geteuid() == 0 {
        return "/var/lib/lxc".to_string();
    }
    if let Some(xdg) = get_env("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return format!("{}/lxc", xdg.trim_end_matches('/'));
        }
    }
    if let Some(home) = get_env("HOME") {
        if !home.is_empty() {
            return format!("{}/.local/share/lxc", home.trim_end_matches('/'));
        }
    }
    "/var/lib/lxc".to_string()
}

pub fn resolve_default_lxcpath() -> String {
    // The windows-latest clippy lane compiles this and never calls it, so a
    // non-root EUID stands in.
    #[cfg(target_os = "linux")]
    // SAFETY: `geteuid` is a thread-safe, side-effect-free libc call.
    fn current_euid() -> u32 {
        unsafe { libc::geteuid() as u32 }
    }
    #[cfg(not(target_os = "linux"))]
    fn current_euid() -> u32 {
        1
    }

    resolve_lxcpath_with_env(|k| std::env::var(k).ok(), current_euid)
}

#[cfg(test)]
fn build_attach_args(env: &[String], working_directory: &str, command: &str) -> Vec<String> {
    build_attach_args_with_env_control(env, working_directory, command, false)
}

/// An empty `env` is ambiguous: the caller expressed no opinion, or a scrub
/// removed every entry there was.  `force_clear_env` distinguishes them; only
/// the second must still shut the host environment out.
#[cfg(any(target_os = "linux", test))]
fn build_attach_args_with_env_control(
    env: &[String],
    working_directory: &str,
    command: &str,
    force_clear_env: bool,
) -> Vec<String> {
    let mut args: Vec<String> = Vec::with_capacity(env.len() + 8);

    if force_clear_env || !env.is_empty() {
        args.push("--clear-env".to_string());
        for kv in env {
            if let Some((key, _)) = kv.split_once('=') {
                if !key.is_empty() {
                    args.push(format!("--set-var={}", kv));
                }
            }
        }
    }

    args.push("--".to_string());
    args.push("/bin/sh".to_string());
    args.push("-c".to_string());

    if working_directory.is_empty() {
        args.push(command.to_string());
    } else {
        args.push("cd -- \"$1\" && exec /bin/sh -c \"$2\"".to_string());
        args.push("_".to_string());
        args.push(working_directory.to_string());
        args.push(command.to_string());
    }

    args
}

/// Whether MXC put firewall chains in the container's network namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerFirewall {
    Installed,
    Absent,
}

#[cfg(target_os = "linux")]
fn confine_network_capabilities(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // `libc` does not export this; the value is from linux/capability.h.
    const CAP_NET_ADMIN: libc::c_ulong = 12;

    // SAFETY: `pre_exec` runs between fork and exec, where only
    // async-signal-safe work is permitted. `prctl` is a bare syscall and this
    // closure allocates nothing and captures nothing.
    unsafe {
        command.pre_exec(|| {
            if libc::prctl(libc::PR_CAPBSET_DROP, CAP_NET_ADMIN, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// A `/etc/hosts` helper emits one `mxc:` diagnostic line, so this is far above
/// any legitimate output.
#[cfg(target_os = "linux")]
const MAX_CAPTURED_BYTES: u64 = 8 * 1024;

#[cfg(target_os = "linux")]
fn read_to_end_on_thread(
    reader: Option<wxc_common::interruptible_reader::InterruptibleReader>,
) -> Option<std::thread::JoinHandle<String>> {
    reader.map(|reader| {
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let mut capped = std::io::Read::take(reader, MAX_CAPTURED_BYTES);
            let _ = std::io::Read::read_to_end(&mut capped, &mut buffer);

            // Drained past the cap so the child sees its pipe emptied and exits
            // on its own rather than blocking on a full one.
            let mut reader = capped.into_inner();
            let _ = std::io::copy(&mut reader, &mut std::io::sink());

            String::from_utf8_lossy(&buffer).into_owned()
        })
    })
}

#[cfg(target_os = "linux")]
fn joined_capture(handle: Option<std::thread::JoinHandle<String>>) -> String {
    handle
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

/// A reaped child's pipes are closed, so a reader still running past this grace
/// is waiting on a descendant that inherited one.
#[cfg(target_os = "linux")]
const DRAIN_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

/// Cancelling a read abandons whatever the pipe still held, so a child whose
/// output has not been read yet would lose it to a prompt cancel.
#[cfg(target_os = "linux")]
fn wait_for_drain(
    stdout: &Option<std::thread::JoinHandle<String>>,
    stderr: &Option<std::thread::JoinHandle<String>>,
) {
    let drained = |handle: &Option<std::thread::JoinHandle<String>>| {
        handle.as_ref().is_none_or(|handle| handle.is_finished())
    };
    let deadline = std::time::Instant::now() + DRAIN_GRACE;

    while !(drained(stdout) && drained(stderr)) {
        if std::time::Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Unblock the signals `lxc-exec` holds for its sigwait watchdog, so the child
/// does not inherit a mask that makes it ignore Ctrl-C and termination.
#[cfg(target_os = "linux")]
fn unblock_fatal_signals(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // SAFETY: `pre_exec` runs between fork and exec, where only
    // async-signal-safe work is permitted. `pthread_sigmask`, which nix's
    // `thread_unblock` wraps, is async-signal-safe, and this closure allocates
    // nothing and captures nothing.
    unsafe {
        command.pre_exec(|| {
            let mut mask = nix::sys::signal::SigSet::empty();
            mask.add(nix::sys::signal::Signal::SIGHUP);
            mask.add(nix::sys::signal::Signal::SIGTERM);
            mask.add(nix::sys::signal::Signal::SIGINT);
            mask.thread_unblock().map_err(std::io::Error::from)?;
            Ok(())
        });
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartNetwork {
    FromContainerConfig,
    NoInterface,
}

impl StartNetwork {
    fn to_start_args(self, configured_interfaces: usize) -> Vec<String> {
        match self {
            StartNetwork::FromContainerConfig => Vec::new(),
            StartNetwork::NoInterface => {
                // Index 0 is emitted even for a container that configures no
                // interface at all: without an `lxc.net` entry LXC leaves the
                // container in the host's network namespace.
                let mut args = Vec::new();
                for index in 0..configured_interfaces.max(1) {
                    args.push("-s".to_string());
                    args.push(format!("lxc.net.{index}.type=empty"));
                    args.push("-s".to_string());
                    args.push(format!("lxc.net.{index}.flags=up"));
                }
                args
            }
        }
    }
}

pub struct LxcContainer {
    name: String,
    lxc_path: String,
}

impl LxcContainer {
    pub fn new(name: &str, lxc_path: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            lxc_path: lxc_path
                .map(|s| s.to_string())
                .unwrap_or_else(resolve_default_lxcpath),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn lxc_path(&self) -> &str {
        &self.lxc_path
    }

    fn lxc_command(&self, tool: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new(tool);
        cmd.arg("-P").arg(&self.lxc_path).arg("-n").arg(&self.name);
        cmd
    }

    fn run_tool(mut cmd: std::process::Command) -> Result<(), String> {
        let tool = cmd.get_program().to_string_lossy().into_owned();
        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run {}: {}", tool, e))?;
        if !output.status.success() {
            return Err(format!(
                "{} failed: {}",
                tool,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    pub fn is_defined(&self) -> bool {
        let output = self.lxc_command("lxc-info").output();
        matches!(output, Ok(o) if o.status.success())
    }

    pub fn is_running(&self) -> bool {
        let output = self.lxc_command("lxc-info").arg("-s").output();
        match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).contains("RUNNING"),
            Err(_) => false,
        }
    }

    pub fn init_pid(&self) -> Option<u32> {
        let output = self.lxc_command("lxc-info").arg("-p").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let token = line.trim();
            let token = token.strip_prefix("PID:").map(str::trim).unwrap_or(token);
            if let Ok(pid) = token.parse::<u32>() {
                if pid > 0 {
                    return Some(pid);
                }
            }
        }
        None
    }

    pub fn create(&self, distribution: &str, release: &str) -> Result<(), String> {
        let mut cmd = self.lxc_command("lxc-create");
        cmd.args(["-t", "download", "--", "-d"])
            .arg(distribution)
            .arg("-r")
            .arg(release)
            .arg("-a")
            .arg(Self::current_arch());
        Self::run_tool(cmd)
    }

    /// Removes existing mount points so they do not accumulate on a reused
    /// container.
    pub fn set_filesystem_access_points(&self, entries: &[String]) -> Result<(), String> {
        let config_path = self.config_file_path();
        let existing = std::fs::read_to_string(&config_path).map_err(|e| {
            format!(
                "Failed to read container config to replace its mount entries: {} (config file: {})",
                e, config_path
            )
        })?;

        let mut out = Self::strip_managed_mount_entries(&existing);
        if !entries.is_empty() {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(MANAGED_MOUNTS_BEGIN);
            out.push('\n');
            for entry in entries {
                out.push_str("lxc.mount.entry = ");
                out.push_str(entry);
                out.push('\n');
            }
            out.push_str(MANAGED_MOUNTS_END);
            out.push('\n');
        }

        let temp_path = format!("{}.mxc-tmp", config_path);
        std::fs::write(&temp_path, out.as_bytes()).map_err(|e| {
            format!(
                "Failed to stage rewritten container config: {} (temp file: {})",
                e, temp_path
            )
        })?;
        std::fs::rename(&temp_path, &config_path).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            format!(
                "Failed to install rewritten container config: {} (config file: {})",
                e, config_path
            )
        })
    }

    fn strip_managed_mount_entries(config: &str) -> String {
        let mut out = String::with_capacity(config.len());
        let mut inside = false;
        for line in config.lines() {
            let trimmed = line.trim();
            if trimmed == MANAGED_MOUNTS_BEGIN {
                inside = true;
                continue;
            }
            if trimmed == MANAGED_MOUNTS_END {
                inside = false;
                continue;
            }
            if inside {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    pub fn start(&self, network: StartNetwork) -> Result<(), String> {
        let mut cmd = self.lxc_command("lxc-start");
        cmd.args(network.to_start_args(self.configured_interface_count()));
        Self::run_tool(cmd)
    }

    /// How many `lxc.net.N` interfaces the container's config declares.
    fn configured_interface_count(&self) -> usize {
        let Ok(config) = std::fs::read_to_string(self.config_file_path()) else {
            return 0;
        };
        Self::highest_interface_index(&config).map_or(0, |index| index + 1)
    }

    fn highest_interface_index(config: &str) -> Option<usize> {
        config
            .lines()
            .filter_map(|line| {
                let key = line.split('=').next()?.trim();
                let index = key.strip_prefix("lxc.net.")?.split('.').next()?;
                index.parse::<usize>().ok()
            })
            .max()
    }

    pub fn exec(
        &self,
        command: &str,
        _working_directory: &str,
        _timeout_ms: u32,
    ) -> Result<(i32, String, String), String> {
        // TODO: Implement timeout and working directory support.
        let mut cmd = self.lxc_command("lxc-execute");
        cmd.args(["--", "/bin/sh", "-c", command]);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run lxc-execute: {}", e))?;

        Ok((
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ))
    }

    /// Output streams go straight to the host; both returned strings are always
    /// empty.
    #[cfg(target_os = "linux")]
    pub fn attach_run(
        &self,
        command: &str,
        working_directory: &str,
        env: &[String],
        force_clear_env: bool,
        timeout: Option<std::time::Duration>,
        firewall: ContainerFirewall,
    ) -> Result<(i32, String, String), String> {
        use mxc_pty::{run_with_pty, PtyOptions, PtyOutcome, Signal};

        // This process blocks these for its cleanup watchdog; left blocked, the
        // inner shell would ignore Ctrl-C.
        const UNBLOCK: &[Signal] = &[Signal::SIGHUP, Signal::SIGTERM, Signal::SIGINT];

        let mut cmd = self.lxc_command("lxc-attach");
        cmd.args(build_attach_args_with_env_control(
            env,
            working_directory,
            command,
            force_clear_env,
        ));

        // The drop needs CAP_SETPCAP, which an unprivileged caller lacks, and a
        // run with no chains has nothing to protect anyway.
        if firewall == ContainerFirewall::Installed {
            confine_network_capabilities(&mut cmd);
        }

        let options = PtyOptions {
            unblock_signals: UNBLOCK,
            timeout,
            ..PtyOptions::default()
        };

        match run_with_pty(cmd, options)? {
            PtyOutcome::Exited(status) => {
                Ok((status.code().unwrap_or(-1), String::new(), String::new()))
            }

            PtyOutcome::TimedOut => {
                let ms = timeout.map(|d| d.as_millis()).unwrap_or(0);
                Err(format!("script timed out after {}ms", ms))
            }
        }
    }

    /// Stub for the workspace-wide clippy lane that runs on Windows.
    #[cfg(not(target_os = "linux"))]
    pub fn attach_run(
        &self,
        _command: &str,
        _working_directory: &str,
        _env: &[String],
        _force_clear_env: bool,
        _timeout: Option<std::time::Duration>,
        _firewall: ContainerFirewall,
    ) -> Result<(i32, String, String), String> {
        Err("LxcContainer::attach_run is only supported on Linux".to_string())
    }

    /// Run a command in the container and return its output, with no pty and no
    /// path from that output to this process's own stdio.
    #[cfg(target_os = "linux")]
    pub fn attach_capture(
        &self,
        command: &str,
        working_directory: &str,
        env: &[String],
        force_clear_env: bool,
        timeout: Option<std::time::Duration>,
        firewall: ContainerFirewall,
    ) -> Result<(i32, String, String), String> {
        use std::process::Stdio;
        use wxc_common::interruptible_reader::wrap_pipe;
        use wxc_common::sandbox_process::{wait_with_timeout, StreamCloser, WaitError};

        let mut cmd = self.lxc_command("lxc-attach");
        cmd.args(build_attach_args_with_env_control(
            env,
            working_directory,
            command,
            force_clear_env,
        ));
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        unblock_fatal_signals(&mut cmd);

        if firewall == ContainerFirewall::Installed {
            confine_network_capabilities(&mut cmd);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to run lxc-attach: {}", e))?;

        let wrapped = wrap_pipe(child.stdout.take())
            .and_then(|out| wrap_pipe(child.stderr.take()).map(|err| (out, err)));
        let ((stdout, stdout_canceller), (stderr, stderr_canceller)) = match wrapped {
            Ok(pipes) => pipes,

            // Without this the helper keeps running, and can still rewrite
            // `/etc/hosts` after this call has reported failure.
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Failed to wrap an lxc-attach pipe: {}", e));
            }
        };
        let stdout = read_to_end_on_thread(stdout);
        let stderr = read_to_end_on_thread(stderr);

        let outcome = wait_with_timeout(&mut child, timeout);
        if outcome.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        } else {
            wait_for_drain(&stdout, &stderr);
        }

        // A descendant holding either pipe open would park the joins below
        // indefinitely.
        for canceller in [stdout_canceller, stderr_canceller].into_iter().flatten() {
            canceller.close();
        }
        let captured_out = joined_capture(stdout);
        let captured_err = joined_capture(stderr);

        let status = match outcome {
            Ok(status) => status,
            Err(WaitError::Timeout) => {
                let ms = timeout.map(|d| d.as_millis()).unwrap_or(0);
                return Err(format!("lxc-attach timed out after {}ms", ms));
            }
            Err(WaitError::Io(e)) => {
                return Err(format!("Failed to wait for lxc-attach: {}", e));
            }
        };

        Ok((status.code().unwrap_or(-1), captured_out, captured_err))
    }

    /// Stub for the workspace-wide clippy lane that runs on Windows.
    #[cfg(not(target_os = "linux"))]
    pub fn attach_capture(
        &self,
        _command: &str,
        _working_directory: &str,
        _env: &[String],
        _force_clear_env: bool,
        _timeout: Option<std::time::Duration>,
        _firewall: ContainerFirewall,
    ) -> Result<(i32, String, String), String> {
        Err("LxcContainer::attach_capture is only supported on Linux".to_string())
    }

    /// Stop the container by killing it, not by asking it to exit.
    pub fn stop(&self) -> Result<(), String> {
        Self::run_tool(self.stop_command())
    }

    fn stop_command(&self) -> std::process::Command {
        let mut cmd = self.lxc_command("lxc-stop");

        // -k kills outright.  Asking it to exit instead waits 60 seconds for a
        // SIGPWR reply that systemd as PID 1 in an unprivileged userns never
        // sends.
        cmd.arg("-k");
        cmd
    }

    pub fn destroy(&self) -> Result<(), String> {
        let mut cmd = self.lxc_command("lxc-destroy");

        cmd.arg("-f");
        Self::run_tool(cmd)
    }

    fn config_file_path(&self) -> String {
        format!("{}/{}/config", self.lxc_path, self.name)
    }

    /// Stops the guest DHCP client from running duplicate address detection on
    /// the address it is offered.
    ///
    /// `dhcpcd` follows RFC 5227 and defends an offered address with ARP
    /// probes before it assigns it, which delays the address by several
    /// seconds. The bridge's DHCP server owns the lease database for the
    /// subnet and probes each candidate itself before offering it, so the
    /// container's own probe repeats a check that has already been made.
    ///
    /// Only a container attached solely to [`PROBING_DHCP_BRIDGE`] is
    /// configured, because only there has the first check certainly been made.
    /// Images that ship a different DHCP client have no `dhcpcd.conf` and are
    /// left alone. The container must not be running: its root filesystem is
    /// read and written from the host, and a container that is executing can
    /// replace the paths involved.
    pub fn skip_dhcp_duplicate_address_detection(&self) -> Result<(), String> {
        let Ok(config) = std::fs::read_to_string(self.config_file_path()) else {
            return Ok(());
        };
        if !Self::attaches_only_to_probing_bridge(&config) {
            return Ok(());
        }
        let Some(rootfs) = Self::configured_rootfs_path(&config) else {
            return Ok(());
        };
        let Some(directory) = Self::resolve_inside_rootfs(&rootfs, "etc")? else {
            return Ok(());
        };
        let conf_path = directory.join("dhcpcd.conf");

        let Some((metadata, existing)) = Self::read_guest_conf(&conf_path)? else {
            return Ok(());
        };
        let Some(updated) = Self::dhcpcd_conf_skipping_detection(&existing) else {
            return Ok(());
        };

        Self::install_replacement(&directory, &conf_path, &updated, &metadata).map_err(|e| {
            format!(
                "Failed to configure the container's DHCP client: {} (file: {})",
                e,
                conf_path.display()
            )
        })
    }

    /// Whether every interface the container declares attaches to the bridge
    /// whose DHCP server probes an address before it offers it.
    ///
    /// A container created against a host default that names another bridge,
    /// or one kept from an earlier run, can sit on a network whose DHCP server
    /// makes no such check. There the guest's own probe is the only one, and
    /// the container is left to make it.
    fn attaches_only_to_probing_bridge(config: &str) -> bool {
        let mut declares_an_interface = false;
        for line in config.lines() {
            let Some((key, link)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if !key.starts_with("lxc.net.") || !key.ends_with(".link") {
                continue;
            }
            declares_an_interface = true;
            if link.trim() != PROBING_DHCP_BRIDGE {
                return false;
            }
        }
        declares_an_interface
    }

    /// The container's `dhcpcd.conf` and the metadata of the file it was read
    /// from, or `None` for an image that ships no `dhcpcd`.
    fn read_guest_conf(
        conf_path: &std::path::Path,
    ) -> Result<Option<(std::fs::Metadata, String)>, String> {
        let displayed = conf_path.display().to_string();

        let mut file = match Self::open_guest_file(conf_path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            // `O_NOFOLLOW` reports a symbolic link in the final position as a
            // loop rather than opening what it points at.
            Err(e) if Self::is_symlink_refusal(&e) => {
                return Err(format!(
                    "Refusing to configure the container's DHCP client: {} is a symbolic link",
                    displayed
                ));
            }
            Err(e) => {
                return Err(format!(
                    "Failed to open the container's DHCP client configuration: {} (file: {})",
                    e, displayed
                ));
            }
        };

        // Everything below works through this one descriptor, so the file that
        // was inspected is the file that is read.
        let metadata = file.metadata().map_err(|e| {
            format!(
                "Failed to inspect the container's DHCP client configuration: {} (file: {})",
                e, displayed
            )
        })?;
        if !metadata.is_file() {
            return Err(format!(
                "Refusing to configure the container's DHCP client: {} is not a regular file",
                displayed
            ));
        }
        if metadata.len() > DHCPCD_CONF_MAX_LEN {
            return Err(format!(
                "Refusing to configure the container's DHCP client: {} holds {} bytes, more than a \
                 DHCP client configuration is expected to",
                displayed,
                metadata.len()
            ));
        }

        let existing = Self::read_bounded(&mut file).map_err(|e| {
            format!(
                "Failed to read the container's DHCP client configuration: {} (file: {})",
                e, displayed
            )
        })?;
        Ok(Some((metadata, existing)))
    }

    /// Reads the configuration under a bound, so a file that grows between the
    /// size check and the read still cannot exhaust the host's memory.
    fn read_bounded(file: &mut std::fs::File) -> std::io::Result<String> {
        use std::io::Read;

        let mut contents = String::new();
        let read = file
            .by_ref()
            .take(DHCPCD_CONF_MAX_LEN + 1)
            .read_to_string(&mut contents)?;
        if read as u64 > DHCPCD_CONF_MAX_LEN {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "it grew past the size a DHCP client configuration is expected to be",
            ));
        }
        Ok(contents)
    }

    /// Puts `contents` at `target` by renaming a finished file over it.
    ///
    /// The replacement is staged beside the original, inside the container's
    /// own `etc`, and takes the original's mode and ownership so the guest
    /// still reads a file of its own. Renaming publishes it in one step, so a
    /// write that fails part way leaves the original untouched rather than
    /// handing the guest a half-written configuration to boot from.
    fn install_replacement(
        directory: &std::path::Path,
        target: &std::path::Path,
        contents: &str,
        original: &std::fs::Metadata,
    ) -> std::io::Result<()> {
        let staged = directory.join("dhcpcd.conf.mxc-tmp");
        // A run killed between the write and the rename would otherwise leave
        // a file that the exclusive create below refuses to replace.
        let _ = std::fs::remove_file(&staged);

        let result = Self::write_staged(&staged, contents, original)
            .and_then(|()| std::fs::rename(&staged, target));
        if result.is_err() {
            let _ = std::fs::remove_file(&staged);
        }
        result
    }

    fn write_staged(
        path: &std::path::Path,
        contents: &str,
        original: &std::fs::Metadata,
    ) -> std::io::Result<()> {
        use std::io::Write;

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }

        let mut file = options.open(path)?;
        file.set_permissions(original.permissions())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            // Staging as the user who owns the original is the ordinary case,
            // and asking to become its owner needs a privilege this may lack.
            let staged = file.metadata()?;
            if staged.uid() != original.uid() || staged.gid() != original.gid() {
                std::os::unix::fs::fchown(&file, Some(original.uid()), Some(original.gid()))?;
            }
        }
        file.write_all(contents.as_bytes())?;
        // The rename may only publish bytes that have reached the disk.
        file.sync_all()
    }

    fn open_guest_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // `O_NOFOLLOW` refuses a symbolic link left in place of the file,
            // and `O_NONBLOCK` stops a device node or FIFO from stalling the
            // open of a container that is about to start.
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        options.open(path)
    }

    #[cfg(unix)]
    fn is_symlink_refusal(error: &std::io::Error) -> bool {
        error.raw_os_error() == Some(libc::ELOOP) || error.raw_os_error() == Some(libc::EMLINK)
    }

    #[cfg(not(unix))]
    fn is_symlink_refusal(_error: &std::io::Error) -> bool {
        false
    }

    /// Resolves `relative` beneath the container's root filesystem, refusing a
    /// path that leaves it.
    ///
    /// The guest owns everything under its root and can replace a directory
    /// with a link to one of the host's. Resolving the whole path and checking
    /// where it lands keeps a link inside the container, where it is ordinary,
    /// from reaching the host's own configuration.
    fn resolve_inside_rootfs(
        rootfs: &str,
        relative: &str,
    ) -> Result<Option<std::path::PathBuf>, String> {
        let root = std::fs::canonicalize(rootfs).map_err(|e| {
            format!(
                "Failed to resolve the container's root filesystem: {} (directory: {})",
                e, rootfs
            )
        })?;
        let resolved = match std::fs::canonicalize(root.join(relative)) {
            Ok(path) => path,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(format!(
                    "Failed to resolve {} inside the container's root filesystem: {}",
                    relative, e
                ));
            }
        };

        if !resolved.starts_with(&root) {
            return Err(format!(
                "Refusing to read {} inside the container: it resolves to {}, outside the \
                 container's root filesystem",
                relative,
                resolved.display()
            ));
        }
        Ok(Some(resolved))
    }

    /// The `dhcpcd.conf` rewritten so its global section skips duplicate
    /// address detection, or `None` when the file must be left as it is.
    ///
    /// An option only applies to the `interface`, `profile` or `ssid` section
    /// it follows, so the option is placed ahead of the first section rather
    /// than at the end of the file, where it would bind to whichever section
    /// happens to be last.
    fn dhcpcd_conf_skipping_detection(existing: &str) -> Option<String> {
        if Self::configures_a_static_address(existing) {
            return None;
        }

        let boundary = Self::first_section_offset(existing);
        let global = &existing[..boundary];
        if global.lines().any(|line| line.trim() == "noarp") {
            return None;
        }

        let mut out = String::with_capacity(existing.len() + DHCPCD_CONF_NOARP.len() + 1);
        out.push_str(global);
        if !global.is_empty() && !global.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(DHCPCD_CONF_NOARP);
        out.push_str(&existing[boundary..]);
        Some(out)
    }

    /// Whether the configuration assigns an address itself instead of taking
    /// one from the DHCP server.
    ///
    /// The bridge's DHCP server never offered, and so never probed, an address
    /// the image sets by hand. Skipping detection is only safe where that
    /// first check was made, so such an image keeps its own.
    fn configures_a_static_address(config: &str) -> bool {
        config.lines().any(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("static")
                && words
                    .next()
                    .is_some_and(|option| option.starts_with("ip_address"))
        })
    }

    /// Where the first `interface`, `profile` or `ssid` section begins, or the
    /// end of the file when it declares none.
    fn first_section_offset(config: &str) -> usize {
        let mut offset = 0;
        for line in config.split_inclusive('\n') {
            let keyword = line.split_whitespace().next().unwrap_or("");
            if matches!(keyword, "interface" | "profile" | "ssid") {
                return offset;
            }
            offset += line.len();
        }
        config.len()
    }

    /// The container's root directory on the host, for the backing stores that
    /// expose one.
    fn configured_rootfs_path(config: &str) -> Option<String> {
        // LXC lets a later assignment replace an earlier one.
        let value = config
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "lxc.rootfs.path").then(|| value.trim())
            })
            .next_back()?;

        match value.split_once(':') {
            None => Some(value.to_string()),
            Some(("dir", path)) => Some(path.to_string()),
            // Every other backing store needs to be assembled before its
            // contents are reachable through a path.
            Some(_) => None,
        }
    }

    fn current_arch() -> &'static str {
        #[cfg(target_arch = "x86_64")]
        {
            "amd64"
        }
        #[cfg(target_arch = "aarch64")]
        {
            "arm64"
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            "amd64"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn stop_kills_rather_than_waiting_for_a_clean_shutdown() {
        let container = LxcContainer::new("mxc-stop-test", Some("/var/lib/lxc"));
        let args: Vec<String> = container
            .stop_command()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();

        assert!(
            args.iter().any(|a| a == "mxc-stop-test"),
            "the command must address this container, got {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "-k"),
            "stop must kill the container: a clean shutdown waits 60 seconds for an \
             init that may never answer, got {args:?}"
        );
    }

    #[test]
    fn a_run_with_no_interface_states_that_to_lxc_start() {
        assert_eq!(
            StartNetwork::NoInterface.to_start_args(1),
            ["-s", "lxc.net.0.type=empty", "-s", "lxc.net.0.flags=up"],
            "lxc-start reads each config item from the -s that precedes it, \
             and loopback stays up for a workload that binds 127.0.0.1"
        );
    }

    #[test]
    fn every_configured_interface_is_emptied_not_just_the_first() {
        assert_eq!(
            StartNetwork::NoInterface.to_start_args(3),
            [
                "-s",
                "lxc.net.0.type=empty",
                "-s",
                "lxc.net.0.flags=up",
                "-s",
                "lxc.net.1.type=empty",
                "-s",
                "lxc.net.1.flags=up",
                "-s",
                "lxc.net.2.type=empty",
                "-s",
                "lxc.net.2.flags=up",
            ],
            "an interface nobody names keeps its link, and a policy that permits \
             no network installs no chain to filter it"
        );
    }

    #[test]
    fn a_container_configuring_no_interface_still_gets_one_emptied() {
        assert_eq!(
            StartNetwork::NoInterface.to_start_args(0),
            ["-s", "lxc.net.0.type=empty", "-s", "lxc.net.0.flags=up"],
            "LXC leaves a container with no lxc.net entry in the host's network namespace"
        );
    }

    #[test]
    fn the_interface_count_comes_from_the_highest_index_the_config_names() {
        for (config, expected) in [
            ("", None),
            ("lxc.net.0.type = veth\n", Some(0)),
            ("lxc.net.0.type = veth\nlxc.net.1.type = veth\n", Some(1)),
            ("lxc.net.4.type = veth\nlxc.net.1.type = veth\n", Some(4)),
            ("  lxc.net.2.link = lxcbr0\n", Some(2)),
            // Keys that only look like an interface entry.
            ("lxc.network.0.type = veth\n", None),
            ("lxc.net.x.type = veth\n", None),
        ] {
            assert_eq!(
                LxcContainer::highest_interface_index(config),
                expected,
                "config {config:?}"
            );
        }
    }

    #[test]
    fn a_run_that_keeps_the_container_config_states_nothing() {
        assert!(
            StartNetwork::FromContainerConfig
                .to_start_args(2)
                .is_empty(),
            "the container's own config must be left to decide its interfaces"
        );
    }

    #[test]
    fn lxcpath_honors_lxc_path_env() {
        let p = resolve_lxcpath_with_env(
            |k| {
                if k == "LXC_PATH" {
                    Some("/custom/lxc".into())
                } else {
                    None
                }
            },
            || 1000,
        );
        assert_eq!(p, "/custom/lxc");
    }

    #[test]
    fn lxcpath_lxc_path_takes_precedence_over_root_default() {
        let p = resolve_lxcpath_with_env(
            |k| {
                if k == "LXC_PATH" {
                    Some("/srv/lxc".into())
                } else {
                    None
                }
            },
            || 0,
        );
        assert_eq!(p, "/srv/lxc");
    }

    #[test]
    fn lxcpath_root_default() {
        let p = resolve_lxcpath_with_env(no_env, || 0);
        assert_eq!(p, "/var/lib/lxc");
    }

    #[test]
    fn lxcpath_user_uses_xdg_data_home() {
        let p = resolve_lxcpath_with_env(
            |k| match k {
                "XDG_DATA_HOME" => Some("/home/u/.data".into()),
                "HOME" => Some("/home/u".into()),
                _ => None,
            },
            || 1000,
        );
        assert_eq!(p, "/home/u/.data/lxc");
    }

    #[test]
    fn lxcpath_user_strips_trailing_slash_on_xdg() {
        let p = resolve_lxcpath_with_env(
            |k| {
                if k == "XDG_DATA_HOME" {
                    Some("/home/u/.data/".into())
                } else {
                    None
                }
            },
            || 1000,
        );
        assert_eq!(p, "/home/u/.data/lxc");
    }

    #[test]
    fn lxcpath_user_falls_back_to_home() {
        let p = resolve_lxcpath_with_env(
            |k| {
                if k == "HOME" {
                    Some("/home/u".into())
                } else {
                    None
                }
            },
            || 1000,
        );
        assert_eq!(p, "/home/u/.local/share/lxc");
    }

    #[test]
    fn lxcpath_user_strips_trailing_slash_on_home() {
        let p = resolve_lxcpath_with_env(
            |k| {
                if k == "HOME" {
                    Some("/home/u/".into())
                } else {
                    None
                }
            },
            || 1000,
        );
        assert_eq!(p, "/home/u/.local/share/lxc");
    }

    #[test]
    fn lxcpath_empty_env_values_are_ignored() {
        let p = resolve_lxcpath_with_env(
            |k| match k {
                "LXC_PATH" | "XDG_DATA_HOME" => Some(String::new()),
                "HOME" => Some("/h".into()),
                _ => None,
            },
            || 1000,
        );
        assert_eq!(p, "/h/.local/share/lxc");
    }

    #[test]
    fn lxcpath_user_with_no_env_has_safe_fallback() {
        let p = resolve_lxcpath_with_env(no_env, || 1000);
        assert_eq!(p, "/var/lib/lxc");
    }

    #[test]
    fn lxc_container_uses_resolved_lxcpath_when_none_provided() {
        let c = LxcContainer::new("any", None);
        assert!(!c.lxc_path().is_empty());
    }

    #[test]
    fn lxc_container_honors_explicit_lxc_path() {
        let c = LxcContainer::new("my-box", Some("/opt/lxc"));
        assert_eq!(c.lxc_path(), "/opt/lxc");
        assert_eq!(c.config_file_path(), "/opt/lxc/my-box/config");
    }

    #[test]
    fn config_file_path_uses_resolved_path() {
        let c = LxcContainer::new("box", Some("/var/lib/lxc"));
        assert_eq!(c.config_file_path(), "/var/lib/lxc/box/config");
    }

    fn container_with_config(body: &str) -> (tempfile::TempDir, LxcContainer, std::path::PathBuf) {
        let base = tempfile::Builder::new()
            .prefix("mxc-lxc-cfg-")
            .tempdir()
            .expect("create temp LXC directory");
        let dir = base.path().join("box");
        std::fs::create_dir_all(&dir).expect("temp container dir");
        let config = dir.join("config");
        std::fs::write(&config, body).expect("seed config");
        let container = LxcContainer::new(
            "box",
            Some(base.path().to_str().expect("temp path must be UTF-8")),
        );
        (base, container, config)
    }

    const TEMPLATE_CONFIG: &str = "# Template used to create this container\n\
                                   lxc.include = /usr/share/lxc/config/common.conf\n\
                                   lxc.rootfs.path = dir:/var/lib/lxc/box/rootfs\n\
                                   lxc.mount.entry = /opt/handwritten opt none bind 0 0\n";

    #[test]
    fn a_reused_container_does_not_inherit_an_earlier_runs_mounts() {
        let (_temp_dir, container, config) = container_with_config(TEMPLATE_CONFIG);

        container
            .set_filesystem_access_points(&[
                "/tmp/secret tmp/secret none bind,create=dir 0 0".into()
            ])
            .expect("first run programs its mount");
        let after_first = std::fs::read_to_string(&config).expect("read config");
        assert!(
            after_first.contains("/tmp/secret"),
            "the first run's mount must be programmed; got:\n{after_first}"
        );

        container
            .set_filesystem_access_points(&[])
            .expect("second run grants nothing");
        let after_second = std::fs::read_to_string(&config).expect("read config");
        assert!(
            !after_second.contains("/tmp/secret"),
            "a run granting no filesystem policy must not inherit the earlier mount; got:\n{after_second}"
        );
    }

    #[test]
    fn rewriting_mounts_preserves_every_line_the_backend_does_not_own() {
        let (_temp_dir, container, config) = container_with_config(TEMPLATE_CONFIG);

        container
            .set_filesystem_access_points(&["/data data none bind,create=dir 0 0".into()])
            .expect("program mounts");
        container
            .set_filesystem_access_points(&[])
            .expect("clear mounts");

        let body = std::fs::read_to_string(&config).expect("read config");
        for line in [
            "# Template used to create this container",
            "lxc.include = /usr/share/lxc/config/common.conf",
            "lxc.rootfs.path = dir:/var/lib/lxc/box/rootfs",
            "lxc.mount.entry = /opt/handwritten opt none bind 0 0",
        ] {
            assert!(
                body.contains(line),
                "{line:?} must survive the rewrite; got:\n{body}"
            );
        }
    }

    #[test]
    fn an_unmarked_mount_is_left_to_its_author() {
        // Containers predating the managed block carry MXC's own mounts as
        // unmarked lines, but so does anyone who wrote one by hand, and the two
        // are indistinguishable. Deleting a user's mount is worse than leaving
        // a stale grant on a container MXC has not rewritten since.
        let unmarked = "lxc.rootfs.path = dir:/var/lib/lxc/box/rootfs\n\
                        lxc.mount.entry = /srv/mydata srv/mydata none bind,create=dir 0 0\n";
        let (_temp_dir, container, config) = container_with_config(unmarked);

        container
            .set_filesystem_access_points(&[])
            .expect("a run granting nothing rewrites the config");

        let body = std::fs::read_to_string(&config).expect("read config");
        assert!(
            body.contains("lxc.mount.entry = /srv/mydata srv/mydata none bind,create=dir 0 0"),
            "an unmarked mount must survive; got:\n{body}"
        );
        assert!(
            body.contains("lxc.rootfs.path = dir:/var/lib/lxc/box/rootfs"),
            "the rewrite must keep the lines it does not own; got:\n{body}"
        );
    }

    #[test]
    fn repeated_runs_do_not_accumulate_managed_blocks() {
        let (_temp_dir, container, config) = container_with_config(TEMPLATE_CONFIG);

        for _ in 0..3 {
            container
                .set_filesystem_access_points(&["/data data none bind,create=dir 0 0".into()])
                .expect("program mounts");
        }

        let body = std::fs::read_to_string(&config).expect("read config");
        assert_eq!(
            body.matches("lxc.mount.entry = /data").count(),
            1,
            "the block must be replaced, not appended; got:\n{body}"
        );
        assert_eq!(
            body.matches(MANAGED_MOUNTS_BEGIN).count(),
            1,
            "exactly one managed block may exist; got:\n{body}"
        );
    }

    #[test]
    fn an_unterminated_managed_block_is_cleared_rather_than_inherited() {
        let truncated = format!(
            "lxc.rootfs.path = dir:/var/lib/lxc/box/rootfs\n{}\nlxc.mount.entry = /tmp/secret tmp/secret none bind 0 0\n",
            MANAGED_MOUNTS_BEGIN
        );
        let (_temp_dir, container, config) = container_with_config(&truncated);

        container
            .set_filesystem_access_points(&[])
            .expect("clear mounts");

        let body = std::fs::read_to_string(&config).expect("read config");
        assert!(
            !body.contains("/tmp/secret"),
            "an unterminated block must not survive; got:\n{body}"
        );
        assert!(
            body.contains("lxc.rootfs.path"),
            "lines before the marker must survive; got:\n{body}"
        );
    }

    #[test]
    fn a_failed_rewrite_leaves_no_temporary_file_behind() {
        let (_temp_dir, container, config) = container_with_config(TEMPLATE_CONFIG);
        let err = LxcContainer::new("ghost", Some("/nonexistent-mxc-base"))
            .set_filesystem_access_points(&[])
            .expect_err("a missing config must fail loudly");
        assert!(
            err.contains("ghost/config"),
            "error must name the config file, got: {err}"
        );

        container
            .set_filesystem_access_points(&["/data data none bind,create=dir 0 0".into()])
            .expect("program mounts");
        let temp = format!("{}.mxc-tmp", config.display());
        assert!(
            !std::path::Path::new(&temp).exists(),
            "the staging file must not outlive a successful rewrite"
        );
    }

    #[test]
    fn build_attach_args_no_env_no_cwd_is_unchanged_legacy_shape() {
        let args = build_attach_args(&[], "", "echo hi");
        assert_eq!(args, vec!["--", "/bin/sh", "-c", "echo hi"]);
    }

    #[test]
    fn build_attach_args_env_is_translated_to_set_var_flags() {
        let env = vec![
            "FOO=bar".to_string(),
            "EMPTY=".to_string(),
            "HAS_EQ_IN_VAL=a=b=c".to_string(),
        ];
        let args = build_attach_args(&env, "", "cmd");
        assert_eq!(
            args,
            vec![
                "--clear-env",
                "--set-var=FOO=bar",
                "--set-var=EMPTY=",
                "--set-var=HAS_EQ_IN_VAL=a=b=c",
                "--",
                "/bin/sh",
                "-c",
                "cmd",
            ]
        );
    }

    #[test]
    fn build_attach_args_env_entries_without_equals_are_skipped() {
        let env = vec!["BADENTRY".to_string(), "OK=val".to_string()];
        let args = build_attach_args(&env, "", "cmd");
        assert_eq!(
            args,
            vec![
                "--clear-env",
                "--set-var=OK=val",
                "--",
                "/bin/sh",
                "-c",
                "cmd",
            ]
        );
    }

    #[test]
    fn build_attach_args_empty_key_entries_are_skipped() {
        let env = vec![
            "=foo".to_string(),
            "=".to_string(),
            "=val=more".to_string(),
            "OK=val".to_string(),
        ];
        let args = build_attach_args(&env, "", "cmd");
        assert_eq!(
            args,
            vec![
                "--clear-env",
                "--set-var=OK=val",
                "--",
                "/bin/sh",
                "-c",
                "cmd",
            ]
        );
    }

    #[test]
    fn build_attach_args_cwd_wraps_command_with_cd_prelude() {
        let args = build_attach_args(&[], "/opt/work", "echo hi");
        assert_eq!(
            args,
            vec![
                "--",
                "/bin/sh",
                "-c",
                "cd -- \"$1\" && exec /bin/sh -c \"$2\"",
                "_",
                "/opt/work",
                "echo hi",
            ]
        );
    }

    #[test]
    fn build_attach_args_cwd_with_special_chars_does_not_require_escaping() {
        let cwd = "/tmp/has spaces & 'quotes' $vars `cmd`";
        let cmd = "printf '%s' \"$PWD\"";
        let args = build_attach_args(&[], cwd, cmd);

        assert_eq!(args[args.len() - 2], cwd);
        assert_eq!(args[args.len() - 1], cmd);

        assert!(args
            .iter()
            .any(|a| a == "cd -- \"$1\" && exec /bin/sh -c \"$2\""));
    }

    #[test]
    fn build_attach_args_combines_env_and_cwd() {
        let env = vec!["FOO=bar".to_string()];
        let args = build_attach_args(&env, "/work", "cmd");
        assert_eq!(
            args,
            vec![
                "--clear-env",
                "--set-var=FOO=bar",
                "--",
                "/bin/sh",
                "-c",
                "cd -- \"$1\" && exec /bin/sh -c \"$2\"",
                "_",
                "/work",
                "cmd",
            ]
        );
    }

    #[test]
    fn build_attach_args_emits_clear_env_when_env_non_empty() {
        let env = vec!["FOO=bar".to_string()];
        let args = build_attach_args(&env, "", "cmd");
        let clear_idx = args
            .iter()
            .position(|a| a == "--clear-env")
            .expect("--clear-env should be present when env is non-empty");
        let set_idx = args
            .iter()
            .position(|a| a == "--set-var=FOO=bar")
            .expect("--set-var entry should be present");
        assert!(
            clear_idx < set_idx,
            "--clear-env must precede --set-var entries, got {:?}",
            args
        );
    }

    #[test]
    fn build_attach_args_omits_clear_env_when_env_empty() {
        let args = build_attach_args(&[], "", "echo hi");
        assert!(
            !args.iter().any(|a| a == "--clear-env"),
            "--clear-env must not appear when env is empty, got {:?}",
            args
        );
    }

    #[test]
    fn build_attach_args_can_force_clear_env_when_env_empty() {
        let args = build_attach_args_with_env_control(&[], "", "cmd", true);
        assert_eq!(args, vec!["--clear-env", "--", "/bin/sh", "-c", "cmd"]);
    }

    #[test]
    fn build_attach_args_clears_env_even_when_all_entries_malformed() {
        let env = vec!["BADENTRY".to_string(), "=alsobad".to_string()];
        let args = build_attach_args(&env, "", "cmd");
        assert_eq!(args, vec!["--clear-env", "--", "/bin/sh", "-c", "cmd"]);
    }

    #[test]
    fn build_attach_args_caller_env_replaces_host_env() {
        let env = vec!["MXC_TEST_FOO=bar baz".to_string()];
        let args = build_attach_args(&env, "", "cmd");
        let clear_idx = args.iter().position(|a| a == "--clear-env").unwrap();
        let set_idx = args
            .iter()
            .position(|a| a == "--set-var=MXC_TEST_FOO=bar baz")
            .unwrap();
        assert!(
            clear_idx < set_idx,
            "--clear-env must precede --set-var so caller value wins, got {:?}",
            args
        );
    }

    #[test]
    fn proxy_disabled_keeps_caller_proxy_env_and_still_clears_inherited_env() {
        use wxc_common::{models::ProxyConfig, proxy_env::apply_proxy_env};
        let mut env = vec![
            "HTTP_PROXY=http://caller-proxy.example:9999".to_string(),
            "PATH=/usr/bin".to_string(),
        ];
        apply_proxy_env(&mut env, &ProxyConfig::default());
        let args = build_attach_args_with_env_control(&env, "", "cmd", true);
        assert!(
            args.iter().any(|a| a == "--clear-env"),
            "the host environment must still be cleared; got {args:?}"
        );
        assert!(
            args.iter()
                .any(|a| a == "--set-var=HTTP_PROXY=http://caller-proxy.example:9999"),
            "a caller's own proxy variable must reach the container; got {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--set-var=PATH=/usr/bin"),
            "PATH must survive; got {args:?}"
        );
    }

    #[test]
    fn proxy_enabled_emits_clear_env_and_proxy_keys_in_attach_args() {
        use wxc_common::{
            models::{ProxyAddress, ProxyConfig},
            proxy_env::apply_proxy_env,
        };
        let proxy = ProxyConfig {
            address: Some(ProxyAddress::new("10.0.0.5".to_string(), 3128)),
            builtin_test_server: false,
        };
        let mut env = vec!["PATH=/usr/bin".to_string()];
        apply_proxy_env(&mut env, &proxy);
        let args = build_attach_args_with_env_control(&env, "", "cmd", true);
        assert!(
            args.iter().any(|a| a == "--clear-env"),
            "proxy enabled must emit --clear-env; got {args:?}"
        );
        assert!(
            args.iter()
                .any(|a| a.starts_with("--set-var=HTTP_PROXY=http://") && a.contains(":3128")),
            "proxy enabled must set HTTP_PROXY (with port 3128); got {args:?}"
        );
        assert!(
            args.iter().any(|a| a == "--set-var=PATH=/usr/bin"),
            "PATH must survive the proxy-env merge; got {args:?}"
        );
    }

    // The attach paths drop this capability only when chains are installed: the
    // drop is privileged, so applying it to every run costs an unprivileged
    // caller the whole execution.
    #[cfg(target_os = "linux")]
    #[test]
    fn confining_a_command_takes_a_privilege_an_unprivileged_caller_lacks() {
        // SAFETY: `geteuid` is a thread-safe, side-effect-free libc call.
        let running_as_root = unsafe { libc::geteuid() } == 0;

        let mut confined = std::process::Command::new("/bin/true");
        confine_network_capabilities(&mut confined);

        assert_eq!(
            confined.status().is_ok(),
            running_as_root,
            "a confined command must spawn only for a caller holding CAP_SETPCAP"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn an_unconfined_command_spawns_whoever_the_caller_is() {
        let mut unconfined = std::process::Command::new("/bin/true");

        assert!(
            unconfined.status().is_ok(),
            "a run with no chains to protect must spawn without any privilege"
        );
    }

    #[test]
    fn a_rootfs_directory_is_read_through_its_backing_store_prefix() {
        assert_eq!(
            LxcContainer::configured_rootfs_path("lxc.rootfs.path = dir:/var/lib/lxc/box/rootfs\n")
                .as_deref(),
            Some("/var/lib/lxc/box/rootfs")
        );
    }

    #[test]
    fn a_rootfs_named_without_a_backing_store_is_taken_as_a_path() {
        assert_eq!(
            LxcContainer::configured_rootfs_path("lxc.rootfs.path = /var/lib/lxc/box/rootfs\n")
                .as_deref(),
            Some("/var/lib/lxc/box/rootfs")
        );
    }

    #[test]
    fn a_rootfs_that_has_to_be_assembled_is_not_addressed_as_a_path() {
        for config in [
            "lxc.rootfs.path = overlayfs:/lower:/upper\n",
            "lxc.rootfs.path = zfs:tank/box\n",
            "lxc.rootfs.path = loop:/var/lib/lxc/box.img\n",
        ] {
            assert_eq!(
                LxcContainer::configured_rootfs_path(config),
                None,
                "{} names a store that is not reachable as a directory",
                config.trim()
            );
        }
    }

    #[test]
    fn the_last_rootfs_assignment_is_the_one_lxc_uses() {
        let config = "lxc.rootfs.path = dir:/first\nlxc.rootfs.path = dir:/second\n";

        assert_eq!(
            LxcContainer::configured_rootfs_path(config).as_deref(),
            Some("/second")
        );
    }

    #[test]
    fn a_config_that_names_no_rootfs_yields_none() {
        assert_eq!(
            LxcContainer::configured_rootfs_path("lxc.net.0.type = veth\n"),
            None
        );
    }

    #[test]
    fn a_dhcp_client_that_already_skips_detection_is_left_alone() {
        for existing in ["noarp\n", "# comment\nnoarp\n", "  noarp  \n"] {
            assert_eq!(
                LxcContainer::dhcpcd_conf_skipping_detection(existing),
                None,
                "{:?} already skips duplicate address detection",
                existing
            );
        }
    }

    #[test]
    fn a_mention_of_the_option_that_does_not_set_it_is_not_mistaken_for_it() {
        for existing in ["#noarp\n", "# noarp\n", "noarp_is_not_this\n"] {
            assert!(
                LxcContainer::dhcpcd_conf_skipping_detection(existing).is_some(),
                "{:?} does not set the option",
                existing
            );
        }
    }

    #[test]
    fn the_option_is_placed_ahead_of_the_first_section() {
        for header in ["interface wlan0", "profile static_eth0", "ssid home"] {
            let existing = format!("hostname\n{}\nmetric 200\n", header);

            let updated = LxcContainer::dhcpcd_conf_skipping_detection(&existing)
                .expect("the global section does not set the option");

            let option = updated.find("\nnoarp\n").expect("the option is written");
            let section = updated.find(header).expect("the section survives");
            assert!(
                option < section,
                "the option must not land inside {:?}, got {:?}",
                header,
                updated
            );
            assert!(
                updated.contains("metric 200"),
                "the section's own options must survive, got {:?}",
                updated
            );
        }
    }

    #[test]
    fn an_option_belonging_to_another_section_does_not_count_as_the_global_one() {
        let existing = "hostname\ninterface wlan0\nnoarp\n";

        let updated = LxcContainer::dhcpcd_conf_skipping_detection(existing)
            .expect("only wlan0 skips detection, so the global section still probes");

        assert!(
            updated.starts_with("hostname\n"),
            "the global section must keep what it had, got {:?}",
            updated
        );
        let option = updated.find("\nnoarp\n").expect("the option is written");
        let section = updated
            .find("interface wlan0")
            .expect("the section survives");
        assert!(
            option < section,
            "the option must be added globally rather than counted from wlan0, got {:?}",
            updated
        );
    }

    #[test]
    fn a_file_that_declares_no_section_is_given_the_option_at_the_end() {
        let updated = LxcContainer::dhcpcd_conf_skipping_detection("hostname\nduid\n")
            .expect("the option is not set");

        assert!(
            updated.starts_with("hostname\nduid\n"),
            "the image's own configuration must survive, got {:?}",
            updated
        );
        assert!(updated.trim_end().ends_with("noarp"));
    }

    #[test]
    fn a_global_section_with_no_trailing_newline_keeps_the_option_on_its_own_line() {
        let updated = LxcContainer::dhcpcd_conf_skipping_detection("hostname")
            .expect("the option is not set");

        assert!(
            updated.starts_with("hostname\n"),
            "the option must not be joined onto the last line, got {:?}",
            updated
        );
        assert!(updated.contains("\nnoarp\n"));
    }

    /// Seeds a container whose rootfs is a real directory, so the DHCP client
    /// configuration can be written and read back.
    fn container_with_rootfs(
        dhcpcd_conf: Option<&str>,
    ) -> (tempfile::TempDir, LxcContainer, std::path::PathBuf) {
        let base = tempfile::Builder::new()
            .prefix("mxc-lxc-rootfs-")
            .tempdir()
            .expect("create temp LXC directory");
        let rootfs = base.path().join("box").join("rootfs");
        std::fs::create_dir_all(rootfs.join("etc")).expect("temp rootfs");
        let conf = rootfs.join("etc").join("dhcpcd.conf");
        if let Some(body) = dhcpcd_conf {
            std::fs::write(&conf, body).expect("seed dhcpcd.conf");
        }
        std::fs::write(
            base.path().join("box").join("config"),
            format!(
                "lxc.rootfs.path = dir:{}\nlxc.net.0.type = veth\nlxc.net.0.link = {}\n",
                rootfs.to_str().expect("temp path must be UTF-8"),
                PROBING_DHCP_BRIDGE
            ),
        )
        .expect("seed config");
        let container = LxcContainer::new(
            "box",
            Some(base.path().to_str().expect("temp path must be UTF-8")),
        );
        (base, container, conf)
    }

    #[test]
    fn a_container_is_told_to_skip_duplicate_address_detection() {
        let (_base, container, conf) = container_with_rootfs(Some("hostname\n"));

        container
            .skip_dhcp_duplicate_address_detection()
            .expect("the option is written");

        let body = std::fs::read_to_string(&conf).expect("read dhcpcd.conf");
        assert!(
            body.lines().any(|line| line.trim() == "noarp"),
            "the option must be set, got {:?}",
            body
        );
        assert!(
            body.starts_with("hostname\n"),
            "the image's own configuration must survive, got {:?}",
            body
        );
    }

    #[test]
    fn a_reused_container_does_not_accumulate_the_option() {
        let (_base, container, conf) = container_with_rootfs(Some("hostname\n"));

        for _ in 0..3 {
            container
                .skip_dhcp_duplicate_address_detection()
                .expect("the option is written");
        }

        let body = std::fs::read_to_string(&conf).expect("read dhcpcd.conf");
        assert_eq!(
            body.lines().filter(|line| line.trim() == "noarp").count(),
            1,
            "a container reused across runs must be configured once, got {:?}",
            body
        );
    }

    #[test]
    fn a_configuration_that_sets_its_own_address_is_left_alone() {
        for existing in [
            "static ip_address=10.0.0.5/24\n",
            "hostname\ninterface eth0\nstatic ip_address=10.0.0.5/24\n",
        ] {
            assert_eq!(
                LxcContainer::dhcpcd_conf_skipping_detection(existing),
                None,
                "{:?} takes no address from the DHCP server, so none was probed for it",
                existing
            );
        }
    }

    #[test]
    fn a_static_option_that_is_not_an_address_still_lets_the_option_be_written() {
        for existing in [
            "static routers=10.0.0.1\n",
            "static domain_name_servers=10.0.0.1\n",
        ] {
            assert!(
                LxcContainer::dhcpcd_conf_skipping_detection(existing).is_some(),
                "{:?} still takes its address from the DHCP server",
                existing
            );
        }
    }

    #[test]
    fn only_a_container_wholly_on_the_probing_bridge_is_configured() {
        for (config, expected) in [
            ("lxc.net.0.link = lxcbr0\n", true),
            ("lxc.net.0.link=lxcbr0\n", true),
            ("  lxc.net.1.link = lxcbr0  \n", true),
            ("lxc.net.0.link = lxcbr0\nlxc.net.1.link = lxcbr0\n", true),
            (
                "lxc.net.0.link = lxcbr0\nlxc.net.1.link = br-custom\n",
                false,
            ),
            ("lxc.net.0.link = br-custom\n", false),
            ("lxc.net.0.type = veth\n", false),
            ("", false),
        ] {
            assert_eq!(
                LxcContainer::attaches_only_to_probing_bridge(config),
                expected,
                "{:?}",
                config
            );
        }
    }

    /// Rewrites the container's config, so a test can attach it elsewhere.
    fn container_attached_to(base: &tempfile::TempDir, interfaces: &str) -> LxcContainer {
        let rootfs = base.path().join("box").join("rootfs");
        std::fs::write(
            base.path().join("box").join("config"),
            format!(
                "lxc.rootfs.path = dir:{}\n{}",
                rootfs.to_str().expect("temp path must be UTF-8"),
                interfaces
            ),
        )
        .expect("rewrite config");
        LxcContainer::new(
            "box",
            Some(base.path().to_str().expect("temp path must be UTF-8")),
        )
    }

    #[test]
    fn a_container_on_another_bridge_keeps_its_own_detection() {
        for interfaces in [
            "lxc.net.0.type = veth\nlxc.net.0.link = br-custom\n",
            "lxc.net.0.link = lxcbr0\nlxc.net.1.link = br-custom\n",
            "",
        ] {
            let (base, _container, conf) = container_with_rootfs(Some("hostname\n"));
            let container = container_attached_to(&base, interfaces);

            container
                .skip_dhcp_duplicate_address_detection()
                .expect("a network that was not measured is not an error");

            assert_eq!(
                std::fs::read_to_string(&conf).expect("read dhcpcd.conf"),
                "hostname\n",
                "a DHCP server that may not probe leaves the guest its own check, got {:?}",
                interfaces
            );
        }
    }

    #[test]
    fn a_configuration_too_large_to_be_one_is_refused() {
        let oversized = "#".repeat(DHCPCD_CONF_MAX_LEN as usize + 1);
        let (_base, container, conf) = container_with_rootfs(Some(&oversized));

        let error = container
            .skip_dhcp_duplicate_address_detection()
            .expect_err("a file this large is not a DHCP client configuration");

        assert!(
            error.contains("more than a DHCP client configuration is expected to"),
            "the refusal must say why, got {:?}",
            error
        );
        assert_eq!(
            std::fs::metadata(&conf).expect("stat dhcpcd.conf").len(),
            DHCPCD_CONF_MAX_LEN + 1,
            "the workload's own file must be left as it is"
        );
    }

    #[test]
    fn a_finished_edit_leaves_no_staged_file_behind() {
        let (_base, container, conf) = container_with_rootfs(Some("hostname\n"));

        container
            .skip_dhcp_duplicate_address_detection()
            .expect("the option is written");

        let stray: Vec<_> = std::fs::read_dir(conf.parent().expect("dhcpcd.conf sits in etc"))
            .expect("read etc")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name())
            .filter(|name| name != "dhcpcd.conf")
            .collect();
        assert!(
            stray.is_empty(),
            "the staged replacement must not outlive the rename, found {:?}",
            stray
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_replacement_keeps_the_mode_the_image_gave_the_file() {
        use std::os::unix::fs::PermissionsExt;

        let (_base, container, conf) = container_with_rootfs(Some("hostname\n"));
        std::fs::set_permissions(&conf, std::fs::Permissions::from_mode(0o640))
            .expect("set the image's mode");

        container
            .skip_dhcp_duplicate_address_detection()
            .expect("the option is written");

        assert_eq!(
            std::fs::metadata(&conf)
                .expect("stat dhcpcd.conf")
                .permissions()
                .mode()
                & 0o777,
            0o640,
            "the guest must still read a file with the mode its image gave it"
        );
    }

    #[test]
    fn rewriting_a_longer_configuration_leaves_nothing_of_the_shorter_one() {
        let (_base, container, conf) =
            container_with_rootfs(Some("hostname\ninterface eth0\nmetric 200\n"));

        container
            .skip_dhcp_duplicate_address_detection()
            .expect("the option is written");

        let body = std::fs::read_to_string(&conf).expect("read dhcpcd.conf");
        assert_eq!(
            body,
            "hostname\n# MXC: the bridge's DHCP server is authoritative for this subnet and\n\
             # probes each address before it offers it.\nnoarp\n\
             interface eth0\nmetric 200\n",
            "the rewrite must leave the file exactly as intended"
        );
    }

    #[test]
    fn an_image_with_a_different_dhcp_client_is_left_alone() {
        let (_base, container, conf) = container_with_rootfs(None);

        container
            .skip_dhcp_duplicate_address_detection()
            .expect("an image without dhcpcd is not an error");

        assert!(
            !conf.exists(),
            "an image that does not ship dhcpcd must not be given a configuration for it"
        );
    }

    #[test]
    fn a_container_whose_rootfs_is_not_a_directory_is_left_alone() {
        let (base, _container, _conf) = container_with_rootfs(Some("hostname\n"));
        std::fs::write(
            base.path().join("box").join("config"),
            "lxc.rootfs.path = overlayfs:/lower:/upper\n",
        )
        .expect("rewrite config");
        let container = LxcContainer::new(
            "box",
            Some(base.path().to_str().expect("temp path must be UTF-8")),
        );

        container
            .skip_dhcp_duplicate_address_detection()
            .expect("a store that is not a directory is not an error");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_dhcp_client_configuration_that_is_a_symlink_is_refused() {
        let (base, container, conf) = container_with_rootfs(None);
        let target = base.path().join("outside");
        std::fs::write(&target, "hostname\n").expect("seed link target");
        std::os::unix::fs::symlink(&target, &conf).expect("link dhcpcd.conf");

        let error = container
            .skip_dhcp_duplicate_address_detection()
            .expect_err("a symbolic link must not be followed out of the rootfs");

        assert!(
            error.contains("symbolic link"),
            "the refusal must say why, got {:?}",
            error
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("read link target"),
            "hostname\n",
            "the file the link points at must be untouched"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_rootfs_whose_etc_leaves_the_container_is_refused() {
        let (base, _container, _conf) = container_with_rootfs(None);
        let outside = base.path().join("host-etc");
        std::fs::create_dir_all(&outside).expect("host etc");
        let host_conf = outside.join("dhcpcd.conf");
        std::fs::write(&host_conf, "hostname\n").expect("seed the host's own configuration");

        let rootfs = base.path().join("box").join("rootfs");
        std::fs::remove_dir_all(rootfs.join("etc")).expect("clear the container's etc");
        std::os::unix::fs::symlink(&outside, rootfs.join("etc")).expect("link etc out");
        let container = LxcContainer::new(
            "box",
            Some(base.path().to_str().expect("temp path must be UTF-8")),
        );

        let error = container
            .skip_dhcp_duplicate_address_detection()
            .expect_err("a directory link that leaves the container must not be followed");

        assert!(
            error.contains("outside the container's root filesystem"),
            "the refusal must say why, got {:?}",
            error
        );
        assert_eq!(
            std::fs::read_to_string(&host_conf).expect("read the host's configuration"),
            "hostname\n",
            "the host's own configuration must be untouched"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_dhcp_client_configuration_that_is_a_fifo_is_refused_rather_than_waited_on() {
        let (_base, container, conf) = container_with_rootfs(None);
        nix::unistd::mkfifo(&conf, nix::sys::stat::Mode::S_IRWXU).expect("make a fifo");

        let error = container
            .skip_dhcp_duplicate_address_detection()
            .expect_err("a fifo must not be read as a configuration file");

        assert!(
            error.contains("not a regular file"),
            "the refusal must say why, got {:?}",
            error
        );
    }
}
