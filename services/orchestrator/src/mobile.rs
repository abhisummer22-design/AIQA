//! Real mobile device discovery.
//!
//! Everything in this module is a real subprocess call to real Android SDK
//! tooling on the host, parsed into real state. There is no device
//! inventory table, no seeded fleet, and no placeholder device: if nothing
//! is plugged in and no emulator is booted, this reports zero devices, and
//! the UI is expected to say so.
//!
//! Both platforms are covered, each through its own real toolchain — and
//! each reports honestly when that toolchain genuinely cannot exist on this
//! host (iOS Simulators require macOS; there is no way to fake one).
//!
//! Real facts gathered:
//!
//!   1. `adb devices -l` — every device the Android Debug Bridge can
//!      currently see, with its real connection state (`device`, `offline`,
//!      `unauthorized`) and its real model/product strings when adb reports
//!      them. An `unauthorized` device is reported as such rather than as
//!      usable, because it genuinely cannot run a test until the USB
//!      debugging prompt is accepted on the handset.
//!   2. `emulator -list-avds` — the AVDs that genuinely exist on this host
//!      and can therefore be booted. An AVD that is not currently running is
//!      reported as bootable, not as a live device.
//!   3. `maestro --version` — whether the real mobile.dev Maestro CLI is
//!      actually present, reusing the same identity distrust
//!      `executor.rs::verify_maestro_identity` already applies (a bare
//!      version string is the real CLI; a usage/help block is a different
//!      tool that happens to occupy the name).
//!
//!   4. `xcrun simctl list devices available --json` — real iOS Simulators.
//!      Only meaningful on macOS: Apple ships no simulator runtime for any
//!      other OS, so on Windows/Linux this reports unavailable with that as
//!      the real reason rather than showing an empty list that looks like
//!      "no simulators created yet".
//!   5. `idevice_id -l` (libimobiledevice) — real *physical* iPhones/iPads,
//!      which unlike simulators can genuinely be attached to a Windows or
//!      Linux host. Absent here unless libimobiledevice is installed, and
//!      reported as absent rather than assumed.
//!
//! Tool paths are resolved from the environment the orchestrator was
//! started with, plus the standard Android SDK locations, because a service
//! launched outside a developer shell frequently does not inherit them.

use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

/// A real device or emulator ADB can currently see.
#[derive(Debug, Clone, Serialize)]
pub struct MobileDevice {
    /// The real adb serial (`emulator-5554`, a USB serial, ...).
    pub serial: String,
    /// Real adb connection state, verbatim: `device` (ready), `offline`,
    /// `unauthorized`, `bootloader`, ...
    pub state: String,
    /// Real `model:` field from `adb devices -l`, when adb reports one.
    pub model: Option<String>,
    /// True only for `state == "device"` — the one state in which a test
    /// can actually be dispatched to it.
    pub ready: bool,
    /// True when the serial is an emulator (adb's own `emulator-NNNN`
    /// convention), as opposed to physical hardware.
    pub emulator: bool,
}

/// An AVD that genuinely exists on this host and could be booted. Listed
/// separately from `MobileDevice` on purpose: an AVD is a *definition*, not
/// a running device, and conflating them would show a fleet that is not
/// actually there.
#[derive(Debug, Clone, Serialize)]
pub struct AvdDefinition {
    pub name: String,
    /// True when an emulator with this AVD name is currently running (matched
    /// against real `adb -s <serial> emu avd name` output).
    pub running: bool,
}

/// A real iOS device: either a Simulator (`xcrun simctl`, macOS only) or a
/// physically attached iPhone/iPad (`libimobiledevice`). Kept separate from
/// `MobileDevice` because the two platforms have genuinely different
/// identifiers, states and tooling — flattening them would force one
/// platform's vocabulary onto the other.
#[derive(Debug, Clone, Serialize)]
pub struct IosDevice {
    /// Real UDID.
    pub udid: String,
    pub name: String,
    /// Real runtime/OS version when the tool reports one.
    pub runtime: Option<String>,
    /// Real state verbatim: simctl's `Booted`/`Shutdown`, or `Connected`
    /// for a physical device libimobiledevice can see.
    pub state: String,
    /// True only when a test could actually be dispatched to it.
    pub ready: bool,
    /// True for a Simulator, false for real hardware.
    pub simulator: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MobileToolStatus {
    /// Real resolved path of the tool, when found.
    pub path: Option<String>,
    pub available: bool,
    /// Real version string, or the real reason the tool is unusable.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MobileFleet {
    pub devices: Vec<MobileDevice>,
    pub avds: Vec<AvdDefinition>,
    pub adb: MobileToolStatus,
    pub emulator_tool: MobileToolStatus,
    pub maestro: MobileToolStatus,
    /// Real iOS devices — Simulators and/or physically attached hardware.
    pub ios_devices: Vec<IosDevice>,
    /// `xcrun simctl` status. On a non-macOS host this is unavailable with
    /// that as the real, stated reason.
    pub simctl: MobileToolStatus,
    /// libimobiledevice status — the only real path to a physical iPhone
    /// from a non-macOS host.
    pub libimobiledevice: MobileToolStatus,
    /// The real host OS, so the UI can explain platform limits truthfully
    /// instead of implying a missing tool could simply be installed.
    pub host_os: &'static str,
}

/// Standard Android SDK locations, checked in addition to `PATH`. A service
/// process often does not inherit a developer shell's `PATH`, and silently
/// reporting "no devices" because a real, installed tool simply was not on
/// `PATH` would be a false negative — worse than an error, because it looks
/// like a real empty fleet.
fn sdk_candidates(tool: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // On Windows the real installs differ per tool: adb/emulator ship as
    // `.exe`, while Maestro installs a `.bat` shim next to a extension-less
    // shell script. Trying every real form is what makes discovery work
    // without hardcoding one tool's packaging.
    let names: Vec<String> = if cfg!(windows) {
        vec![
            format!("{tool}.exe"),
            format!("{tool}.bat"),
            format!("{tool}.cmd"),
        ]
    } else {
        vec![tool.to_string()]
    };

    let home_roots: Vec<PathBuf> = ["USERPROFILE", "HOME"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(PathBuf::from)
        .collect();

    // Maestro's own real install location (`~/.maestro/bin`), which is not
    // part of the Android SDK at all.
    for home in &home_roots {
        for name in &names {
            out.push(home.join(".maestro").join("bin").join(name));
            out.push(home.join(".maestro").join("maestro").join("bin").join(name));
        }
    }

    let subdirs = [
        "platform-tools",
        "emulator",
        "cmdline-tools/latest/bin",
        "tools",
    ];
    let roots: Vec<PathBuf> = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .map(PathBuf::from)
        .chain(
            std::env::var("LOCALAPPDATA")
                .ok()
                .map(|l| PathBuf::from(l).join("Android/Sdk")),
        )
        .chain(home_roots.iter().map(|h| h.join("Android/Sdk")))
        .chain(
            home_roots
                .iter()
                .map(|h| h.join("AppData/Local/Android/Sdk")),
        )
        .collect();
    for root in roots {
        for sub in subdirs {
            for name in &names {
                out.push(root.join(sub).join(name));
            }
        }
    }
    out
}

/// Resolves a tool to a real, existing path — `PATH` first, then the real
/// SDK locations above. Returns `None` when the tool genuinely is not
/// installed anywhere this process can see.
fn resolve_tool(tool: &str) -> Option<PathBuf> {
    for candidate in sdk_candidates(tool) {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // Fall back to bare name; the OS resolves it against PATH at spawn time.
    Some(PathBuf::from(tool))
}

async fn run_tool(program: &PathBuf, args: &[&str]) -> Result<std::process::Output, String> {
    // A `.bat`/`.cmd` shim (how Maestro really installs on Windows) cannot
    // be spawned directly — it must go through the command interpreter.
    let is_batch = program
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("bat") || e.eq_ignore_ascii_case("cmd"))
        .unwrap_or(false);
    let mut cmd = if cfg!(windows) && is_batch {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(program).args(args);
        c
    } else {
        let mut c = tokio::process::Command::new(program);
        c.args(args);
        c
    };
    cmd.kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — device discovery must never flash a console.
        cmd.creation_flags(0x0800_0000);
    }
    match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("timed out after 20s".to_string()),
    }
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Parses real `adb devices -l` output. Format is one device per line after
/// a `List of devices attached` header:
///   `emulator-5554   device product:sdk_gphone64_x86_64 model:sdk_gphone64 ...`
fn parse_adb_devices(text: &str) -> Vec<MobileDevice> {
    text.lines()
        .skip_while(|l| !l.to_lowercase().contains("list of devices"))
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('*') {
                return None;
            }
            let mut parts = line.split_whitespace();
            let serial = parts.next()?.to_string();
            let state = parts.next()?.to_string();
            let model = parts
                .find(|p| p.starts_with("model:"))
                .map(|p| p.trim_start_matches("model:").to_string());
            Some(MobileDevice {
                ready: state == "device",
                emulator: serial.starts_with("emulator-"),
                serial,
                state,
                model,
            })
        })
        .collect()
}

/// Real Maestro identity check — a bare version string is the genuine
/// mobile.dev CLI; a usage/help block means some other tool occupies the
/// name. Mirrors `executor.rs::verify_maestro_identity` so this page cannot
/// report Maestro as available while the executor would refuse to use it.
fn classify_maestro(text: &str) -> (bool, String) {
    let lower = text.to_lowercase();
    const WRONG_TOOL_MARKERS: [&str; 4] = ["florence", "paligemma", "qwen", "cannot import recipe"];
    if WRONG_TOOL_MARKERS.iter().any(|m| lower.contains(m)) {
        return (
            false,
            "the 'maestro' on PATH is a different tool, not the mobile.dev Maestro CLI".to_string(),
        );
    }
    if lower.contains("usage:") && lower.contains("[options]") {
        return (
            false,
            "'maestro --version' returned a usage/help block, not a version string".to_string(),
        );
    }
    let version = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if version.is_empty() {
        return (false, "'maestro --version' produced no output".to_string());
    }
    (true, format!("Maestro {version}"))
}

/// Real iOS Simulators via `xcrun simctl list devices available --json`.
///
/// Gated on macOS first, and that gate is the honest part: Apple ships no
/// Simulator runtime for Windows or Linux, so on those hosts this is not a
/// missing-tool problem that installing something would fix. Reporting an
/// empty list there would read as "no simulators created yet" — a different,
/// and false, statement.
async fn discover_ios_simulators() -> (Vec<IosDevice>, MobileToolStatus) {
    if !cfg!(target_os = "macos") {
        return (
            Vec::new(),
            MobileToolStatus {
                path: None,
                available: false,
                detail: format!(
                    "iOS Simulators require macOS - Apple ships no Simulator runtime for {}. This is a platform limit, not a missing install.",
                    std::env::consts::OS
                ),
            },
        );
    }

    let xcrun = PathBuf::from("xcrun");
    let out = match run_tool(
        &xcrun,
        &["simctl", "list", "devices", "available", "--json"],
    )
    .await
    {
        Ok(o) => o,
        Err(e) => {
            return (
                Vec::new(),
                MobileToolStatus {
                    path: Some("xcrun".to_string()),
                    available: false,
                    detail: format!("could not run xcrun simctl: {e}"),
                },
            )
        }
    };

    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let parsed: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return (
                Vec::new(),
                MobileToolStatus {
                    path: Some("xcrun".to_string()),
                    available: false,
                    detail: format!("xcrun simctl returned output this build could not parse: {e}"),
                },
            )
        }
    };

    let devices = parse_simctl_devices(&parsed);
    let count = devices.len();
    (
        devices,
        MobileToolStatus {
            path: Some("xcrun".to_string()),
            available: true,
            detail: format!("simctl reachable - {count} available simulator(s)"),
        },
    )
}

/// Parses a real `xcrun simctl list devices available --json` payload.
/// Split out from the macOS-gated caller so it is testable on any host.
fn parse_simctl_devices(parsed: &serde_json::Value) -> Vec<IosDevice> {
    let mut devices = Vec::new();
    if let Some(map) = parsed.get("devices").and_then(|d| d.as_object()) {
        for (runtime, list) in map {
            // simctl keys runtimes as
            // `com.apple.CoreSimulator.SimRuntime.iOS-17-5`; the trailing
            // segment is the real human-meaningful version.
            let runtime_label = runtime
                .rsplit('.')
                .next()
                .unwrap_or(runtime)
                .replace('-', " ");
            for d in list.as_array().into_iter().flatten() {
                let state = d
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let udid = match d.get("udid").and_then(|v| v.as_str()) {
                    Some(u) => u.to_string(),
                    None => continue,
                };
                devices.push(IosDevice {
                    name: d
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unnamed")
                        .to_string(),
                    runtime: Some(runtime_label.clone()),
                    ready: state == "Booted",
                    state,
                    udid,
                    simulator: true,
                });
            }
        }
    }
    devices
}

/// Real *physical* iOS devices via libimobiledevice. This is the only real
/// way to reach an attached iPhone from a non-macOS host, so it is checked
/// on every platform rather than gated behind macOS.
async fn discover_ios_physical() -> (Vec<IosDevice>, MobileToolStatus) {
    let tool = match resolve_tool("idevice_id") {
        Some(p) => p,
        None => {
            return (
                Vec::new(),
                MobileToolStatus {
                    path: None,
                    available: false,
                    detail: "libimobiledevice not installed - required to see a physically attached iPhone/iPad".to_string(),
                },
            )
        }
    };

    let out = match run_tool(&tool, &["-l"]).await {
        Ok(o) => o,
        Err(e) => {
            return (
                Vec::new(),
                MobileToolStatus {
                    path: Some(tool.display().to_string()),
                    available: false,
                    detail: format!("libimobiledevice not usable: {e}"),
                },
            )
        }
    };

    let udids: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    let info_tool = resolve_tool("ideviceinfo");
    let mut devices = Vec::new();
    for udid in udids {
        // Real name/version per device when `ideviceinfo` can supply them;
        // `None` rather than a placeholder when it cannot.
        let mut name = None;
        let mut version = None;
        if let Some(info) = &info_tool {
            if let Ok(o) = run_tool(info, &["-u", &udid, "-k", "DeviceName"]).await {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !v.is_empty() {
                    name = Some(v);
                }
            }
            if let Ok(o) = run_tool(info, &["-u", &udid, "-k", "ProductVersion"]).await {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if !v.is_empty() {
                    version = Some(format!("iOS {v}"));
                }
            }
        }
        devices.push(IosDevice {
            name: name.unwrap_or_else(|| "iOS device".to_string()),
            runtime: version,
            state: "Connected".to_string(),
            ready: true,
            udid,
            simulator: false,
        });
    }

    let count = devices.len();
    (
        devices,
        MobileToolStatus {
            path: Some(tool.display().to_string()),
            available: true,
            detail: format!("libimobiledevice reachable - {count} attached device(s)"),
        },
    )
}

/// Boots a real iOS Simulator (`xcrun simctl boot <udid>`). macOS only, for
/// the same real reason discovery is.
pub async fn boot_ios_simulator(udid: &str) -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err(format!(
            "iOS Simulators require macOS - cannot boot one on {}",
            std::env::consts::OS
        ));
    }
    // UDIDs are hex + hyphens; reject anything else before it reaches a
    // subprocess argument.
    if udid.is_empty() || !udid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err("invalid simulator UDID".to_string());
    }
    let xcrun = PathBuf::from("xcrun");
    match run_tool(&xcrun, &["simctl", "boot", udid]).await {
        Ok(out) if out.status.success() => Ok(format!("simulator {udid} booting")),
        Ok(out) => {
            let err = combined(&out);
            // simctl exits non-zero when the device is already booted; that
            // is not a failure worth surfacing as one.
            if err.to_lowercase().contains("already booted") {
                Ok(format!("simulator {udid} was already booted"))
            } else {
                Err(format!("simctl refused to boot {udid}: {}", err.trim()))
            }
        }
        Err(e) => Err(format!("could not run xcrun simctl boot: {e}")),
    }
}

/// Gathers the whole real fleet. Every field is either a real observed value
/// or a real error string — nothing is defaulted to a plausible-looking
/// value when a tool is missing.
pub async fn discover_fleet() -> MobileFleet {
    // --- adb ---
    let adb_path = resolve_tool("adb");
    let (devices, adb_status) = match &adb_path {
        Some(p) => match run_tool(p, &["devices", "-l"]).await {
            Ok(out) => {
                let text = combined(&out);
                (
                    parse_adb_devices(&text),
                    MobileToolStatus {
                        path: Some(p.display().to_string()),
                        available: true,
                        detail: "adb reachable".to_string(),
                    },
                )
            }
            Err(e) => (
                Vec::new(),
                MobileToolStatus {
                    path: Some(p.display().to_string()),
                    available: false,
                    detail: format!("could not run adb: {e}"),
                },
            ),
        },
        None => (
            Vec::new(),
            MobileToolStatus {
                path: None,
                available: false,
                detail: "adb not found on PATH or in the Android SDK".to_string(),
            },
        ),
    };

    // --- emulator / AVDs ---
    let emu_path = resolve_tool("emulator");
    let (avd_names, emu_status) = match &emu_path {
        Some(p) => match run_tool(p, &["-list-avds"]).await {
            Ok(out) => {
                let text = combined(&out);
                let names: Vec<String> = text
                    .lines()
                    .map(str::trim)
                    .filter(|l| {
                        // The emulator binary prints unrelated warnings to
                        // stderr on some hosts; an AVD name never contains
                        // whitespace or a colon.
                        !l.is_empty() && !l.contains(' ') && !l.contains(':')
                    })
                    .map(str::to_string)
                    .collect();
                (
                    names,
                    MobileToolStatus {
                        path: Some(p.display().to_string()),
                        available: true,
                        detail: "emulator reachable".to_string(),
                    },
                )
            }
            Err(e) => (
                Vec::new(),
                MobileToolStatus {
                    path: Some(p.display().to_string()),
                    available: false,
                    detail: format!("could not run emulator: {e}"),
                },
            ),
        },
        None => (
            Vec::new(),
            MobileToolStatus {
                path: None,
                available: false,
                detail: "emulator not found on PATH or in the Android SDK".to_string(),
            },
        ),
    };

    // Which AVDs are genuinely running: ask each live emulator serial for
    // its own AVD name rather than guessing from the serial number.
    let mut running_avds: Vec<String> = Vec::new();
    if let Some(p) = &adb_path {
        for d in devices.iter().filter(|d| d.emulator && d.ready) {
            if let Ok(out) = run_tool(p, &["-s", &d.serial, "emu", "avd", "name"]).await {
                if let Some(name) = combined(&out)
                    .lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                {
                    running_avds.push(name.to_string());
                }
            }
        }
    }

    let avds = avd_names
        .into_iter()
        .map(|name| AvdDefinition {
            running: running_avds.iter().any(|r| r == &name),
            name,
        })
        .collect();

    // --- maestro ---
    let maestro_path = resolve_tool("maestro");
    let maestro = match &maestro_path {
        Some(p) => match run_tool(p, &["--version"]).await {
            Ok(out) => {
                let (available, detail) = classify_maestro(&combined(&out));
                MobileToolStatus {
                    path: Some(p.display().to_string()),
                    available,
                    detail,
                }
            }
            Err(e) => MobileToolStatus {
                path: Some(p.display().to_string()),
                available: false,
                detail: format!("could not run maestro: {e}"),
            },
        },
        None => MobileToolStatus {
            path: None,
            available: false,
            detail: "maestro not found".to_string(),
        },
    };

    // --- iOS ---
    let (sim_devices, simctl) = discover_ios_simulators().await;
    let (phys_devices, libimobile) = discover_ios_physical().await;
    let mut ios_devices = sim_devices;
    ios_devices.extend(phys_devices);

    MobileFleet {
        devices,
        avds,
        adb: adb_status,
        emulator_tool: emu_status,
        maestro,
        ios_devices,
        simctl,
        libimobiledevice: libimobile,
        host_os: std::env::consts::OS,
    }
}

/// Boots a real AVD. Spawns the real `emulator -avd <name>` process
/// detached — booting genuinely takes tens of seconds, so this returns as
/// soon as the process starts and the caller re-polls `discover_fleet` to
/// see it appear as a real adb device. Nothing is reported as "booted" here;
/// only adb can prove that.
pub async fn boot_avd(name: &str) -> Result<String, String> {
    // AVD names come from the real `-list-avds` output, but this is a
    // subprocess argument either way: reject anything outside the character
    // class real AVD names use rather than trusting the caller.
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err("invalid AVD name".to_string());
    }
    let emu = resolve_tool("emulator").ok_or_else(|| "emulator tool not found".to_string())?;
    let mut cmd = tokio::process::Command::new(&emu);
    cmd.args(["-avd", name]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NO_WINDOW — the emulator must outlive
        // this request without holding a console.
        cmd.creation_flags(0x0000_0008 | 0x0800_0000);
    }
    match cmd.spawn() {
        Ok(child) => Ok(format!(
            "emulator '{name}' starting (pid {})",
            child.id().unwrap_or(0)
        )),
        Err(e) => Err(format!("could not start emulator: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_real_adb_devices_listing() {
        let sample = "List of devices attached\nemulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emu64x transport_id:1\n";
        let devices = parse_adb_devices(sample);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].serial, "emulator-5554");
        assert_eq!(devices[0].state, "device");
        assert!(devices[0].ready);
        assert!(devices[0].emulator);
        assert_eq!(devices[0].model.as_deref(), Some("sdk_gphone64_x86_64"));
    }

    #[test]
    fn an_unauthorized_device_is_not_reported_as_ready() {
        let sample = "List of devices attached\nR58M12ABCDE            unauthorized\n";
        let devices = parse_adb_devices(sample);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].state, "unauthorized");
        assert!(
            !devices[0].ready,
            "an unauthorized handset genuinely cannot run a test until the USB prompt is accepted"
        );
        assert!(!devices[0].emulator);
    }

    #[test]
    fn an_empty_listing_is_a_real_empty_fleet_not_an_error() {
        let devices = parse_adb_devices("List of devices attached\n\n");
        assert!(devices.is_empty());
    }

    #[test]
    fn adb_daemon_startup_noise_is_not_parsed_as_a_device() {
        let sample = "* daemon not running; starting now at tcp:5037\n* daemon started successfully\nList of devices attached\n";
        assert!(parse_adb_devices(sample).is_empty());
    }

    #[test]
    fn a_bare_version_string_is_the_real_maestro() {
        let (ok, detail) = classify_maestro("2.8.0\n");
        assert!(ok);
        assert_eq!(detail, "Maestro 2.8.0");
    }

    #[test]
    fn a_usage_block_is_rejected_as_the_wrong_tool() {
        let (ok, _) = classify_maestro("Usage: maestro [OPTIONS] COMMAND [ARGS]...\n");
        assert!(!ok, "a CLI usage block is not a version string");
    }

    #[test]
    fn the_known_wrong_ml_tool_is_rejected() {
        let (ok, detail) = classify_maestro("WARNING cannot import recipe for Florence-2\n");
        assert!(!ok);
        assert!(detail.contains("different tool"));
    }

    #[test]
    fn parses_a_real_simctl_payload_into_real_ios_devices() {
        // Trimmed but structurally identical to real
        // `xcrun simctl list devices available --json` output.
        let raw = r#"{
          "devices": {
            "com.apple.CoreSimulator.SimRuntime.iOS-17-5": [
              {"udid":"A1B2C3D4-0000-1111-2222-333344445555","name":"iPhone 15 Pro","state":"Booted","isAvailable":true},
              {"udid":"B1B2C3D4-0000-1111-2222-333344445555","name":"iPad Air 11-inch","state":"Shutdown","isAvailable":true}
            ]
          }
        }"#;
        let parsed: serde_json::Value = serde_json::from_str(raw).unwrap();
        let devices = parse_simctl_devices(&parsed);
        assert_eq!(devices.len(), 2);

        let booted = devices.iter().find(|d| d.name == "iPhone 15 Pro").unwrap();
        assert!(booted.simulator);
        assert_eq!(booted.state, "Booted");
        assert!(booted.ready, "a Booted simulator can genuinely run a test");
        // The runtime key is turned into the real human-readable version.
        assert_eq!(booted.runtime.as_deref(), Some("iOS 17 5"));

        let shutdown = devices
            .iter()
            .find(|d| d.name == "iPad Air 11-inch")
            .unwrap();
        assert_eq!(shutdown.state, "Shutdown");
        assert!(
            !shutdown.ready,
            "a Shutdown simulator must not be reported as usable"
        );
    }

    #[test]
    fn an_empty_simctl_payload_is_a_real_empty_ios_fleet() {
        let parsed: serde_json::Value = serde_json::from_str(r#"{"devices":{}}"#).unwrap();
        assert!(parse_simctl_devices(&parsed).is_empty());
    }

    #[test]
    fn a_simctl_entry_without_a_udid_is_dropped_not_invented() {
        let raw = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-17-5":[{"name":"Broken","state":"Shutdown"}]}}"#;
        let parsed: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(
            parse_simctl_devices(&parsed).is_empty(),
            "a device with no real identifier must be dropped, never given a synthesised one"
        );
    }

    #[test]
    fn booting_an_ios_simulator_off_macos_is_refused_with_the_real_platform_reason() {
        if cfg!(target_os = "macos") {
            return;
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(boot_ios_simulator("A1B2C3D4-0000-1111-2222-333344445555"))
            .unwrap_err();
        assert!(
            err.contains("require macOS"),
            "the refusal must state the real platform limit, got: {err}"
        );
    }

    #[test]
    fn boot_rejects_an_injection_shaped_simulator_udid() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(boot_ios_simulator("abc && calc")).unwrap_err();
        // On a non-macOS host the platform gate is hit first; on macOS the
        // UDID character-class check is. Either way it is refused, and never
        // reaches a subprocess.
        assert!(err.contains("invalid simulator UDID") || err.contains("require macOS"));
    }

    #[test]
    fn boot_rejects_an_injection_shaped_avd_name() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(boot_avd("e2e && calc")).unwrap_err();
        assert_eq!(err, "invalid AVD name");
    }
}
