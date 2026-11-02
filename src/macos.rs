//! Native macOS integration. All jobs run in the logged-in user's launchd domain.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

fn output(command: &mut Command) -> Result<Output, String> {
    crate::utils::bounded_output(command, Duration::from_secs(30))
        .ok_or_else(|| "команда macOS не завершилась за 30 секунд".to_string())
}

fn checked(command: &mut Command) -> Result<Output, String> {
    let out = output(command)?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn elevated_helper(exe: &Path, flag: &str, install: &Path) -> Result<Output, String> {
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let command = format!(
        "{} {} {}",
        quote(&exe.to_string_lossy()),
        flag,
        quote(&install.to_string_lossy())
    );
    // AppleScript string escaping is separate from shell argument quoting.
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        command.replace('\\', "\\\\").replace('"', "\\\"")
    );
    // A person may need longer than the normal command timeout to enter a password.
    Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| format!("не удалось показать запрос пароля: {e}"))
}

fn is_macho(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0; 4];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && matches!(
            magic,
            [0xcf, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xcf]
                | [0xce, 0xfa, 0xed, 0xfe]
                | [0xfe, 0xed, 0xfa, 0xce]
                | [0xca, 0xfe, 0xba, 0xbe]
                | [0xbe, 0xba, 0xfe, 0xca]
                | [0xca, 0xfe, 0xba, 0xbf]
                | [0xbf, 0xba, 0xfe, 0xca]
        )
}

/// Sign the temporary copy BEFORE replacing a working executable. A signing
/// failure therefore leaves the original intact, on patch and revert alike.
pub fn sign_binary(path: &Path) -> Result<(), String> {
    if !is_macho(path) {
        return Ok(());
    }
    checked(
        Command::new("/usr/bin/codesign")
            .args([
                "--force",
                "--sign",
                "-",
                "--timestamp=none",
                "--preserve-metadata=identifier,entitlements,flags,runtime",
            ])
            .arg(path),
    )
    .map_err(|e| format!("не удалось подписать {}: {e}", path.display()))?;
    checked(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(path),
    )?;
    Ok(())
}

/// Refresh the enclosing resource seal after modifying its contents. Nested
/// executables are signed individually; --deep is deliberately unnecessary.
pub fn resign_bundle(install: &Path) -> Result<(), String> {
    let Some(bundle) = install
        .ancestors()
        .find(|p| p.extension().is_some_and(|x| x == "app"))
    else {
        return Ok(());
    };
    if output(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--strict"])
            .arg(bundle),
    )?
    .status
    .success()
    {
        return Ok(());
    }
    checked(
        Command::new("/usr/bin/codesign")
            .args([
                "--force",
                "--sign",
                "-",
                "--timestamp=none",
                "--preserve-metadata=identifier,entitlements,flags,runtime",
            ])
            .arg(bundle),
    )
    .map_err(|e| format!("подпись {}: {e}", bundle.display()))?;
    Ok(())
}

pub(crate) fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn domain() -> Result<String, String> {
    let out = checked(Command::new("/usr/bin/id").arg("-u"))?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if uid == "0" {
        return Err("запускайте анлокер от своего пользователя, без sudo".into());
    }
    Ok(format!("gui/{uid}"))
}

fn agent_path(label: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/LaunchAgents")
        .join(format!("{label}.plist"))
}

fn agent_loaded(label: &str) -> bool {
    domain()
        .ok()
        .and_then(|d| {
            output(Command::new("/bin/launchctl").args(["print", &format!("{d}/{label}")])).ok()
        })
        .is_some_and(|o| o.status.success())
}

fn unload(label: &str) -> Result<(), String> {
    let target = format!("{}/{label}", domain()?);
    if agent_loaded(label) {
        checked(Command::new("/bin/launchctl").args(["bootout", &target]))?;
    }
    Ok(())
}

fn load_agent(label: &str, args: &[String], keep_alive: bool) -> Result<(), String> {
    let path = agent_path(label);
    let strings = args
        .iter()
        .map(|v| format!("<string>{}</string>", xml(v)))
        .collect::<String>();
    let logs = crate::dns_forwarder::log_dir();
    fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let body = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
        <plist version=\"1.0\"><dict>\n\
        <key>Label</key><string>{}</string>\n\
        <key>ProgramArguments</key><array>{strings}</array>\n\
        <key>EnvironmentVariables</key><dict><key>HOME</key><string>{}</string></dict>\n\
        <key>RunAtLoad</key><true/>\n\
        <key>KeepAlive</key><{}/><key>ThrottleInterval</key><integer>5</integer>\n\
        <key>StandardOutPath</key><string>{}</string>\n\
        <key>StandardErrorPath</key><string>{}</string>\n\
        </dict></plist>\n", xml(label), xml(&std::env::var("HOME").unwrap_or_default()),
        keep_alive, xml(&logs.join(format!("{label}.log")).to_string_lossy()),
        xml(&logs.join(format!("{label}.error.log")).to_string_lossy()));
    let changed = fs::read_to_string(&path).ok().as_deref() != Some(body.as_str());
    let d = domain()?;
    if changed {
        unload(label)?;
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&path, body).map_err(|e| e.to_string())?;
    }
    checked(Command::new("/bin/launchctl").args(["enable", &format!("{d}/{label}")]))?;
    if !agent_loaded(label) {
        checked(
            Command::new("/bin/launchctl")
                .args(["bootstrap", &d])
                .arg(&path),
        )?;
    }
    Ok(())
}

fn remove_agent(label: &str) -> Result<(), String> {
    unload(label)?;
    match fs::remove_file(agent_path(label)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

const ENV_LABEL: &str = "org.agunlocker.environment";

pub fn environment(name: &str) -> Option<String> {
    output(Command::new("/bin/launchctl").args(["getenv", name]))
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn set_proxy(url: &str) -> Result<(), String> {
    let name = crate::endpoint::PROXY_ENV_VAR;
    checked(Command::new("/bin/launchctl").args(["setenv", name, url]))?;
    load_agent(
        ENV_LABEL,
        &[
            "/bin/launchctl".into(),
            "setenv".into(),
            name.into(),
            url.into(),
        ],
        false,
    )
}

pub fn remove_proxy(url: &str) -> Result<(), String> {
    remove_agent(ENV_LABEL)?;
    if environment(crate::endpoint::PROXY_ENV_VAR)
        .is_some_and(|v| crate::endpoint::is_ours_on_any_port(&v, url))
    {
        checked(Command::new("/bin/launchctl").args(["unsetenv", crate::endpoint::PROXY_ENV_VAR]))?;
    }
    Ok(())
}

pub mod background {
    use super::*;
    const PROXY: &str = "org.agunlocker.proxy";
    const WATCHDOG: &str = "org.agunlocker.watchdog";

    pub fn install_dir() -> PathBuf {
        crate::dns_forwarder::log_dir()
    }
    pub fn installed_exe() -> PathBuf {
        install_dir().join("ag_proxy")
    }
    pub fn is_enabled() -> bool {
        agent_path(PROXY).is_file()
    }
    pub fn is_watchdog_enabled() -> bool {
        agent_path(WATCHDOG).is_file()
    }
    pub fn is_running() -> bool {
        domain()
            .ok()
            .and_then(|d| {
                output(Command::new("/bin/launchctl").args(["print", &format!("{d}/{PROXY}")])).ok()
            })
            .is_some_and(|o| {
                o.status.success()
                    && String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .any(|l| l.trim() == "state = running")
            })
    }
    pub fn relay_is_outdated() -> bool {
        false
    }

    fn install_copy() -> Result<(), String> {
        let src = std::env::current_exe().map_err(|e| e.to_string())?;
        let dst = installed_exe();
        if src == dst
            || fs::read(&src)
                .ok()
                .zip(fs::read(&dst).ok())
                .is_some_and(|(a, b)| a == b)
        {
            return Ok(());
        }
        fs::create_dir_all(install_dir()).map_err(|e| e.to_string())?;
        let tmp = dst.with_extension(format!("{}.tmp", std::process::id()));
        fs::copy(&src, &tmp).map_err(|e| e.to_string())?;
        if let Err(e) = fs::rename(&tmp, &dst) {
            let _ = fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        for label in [PROXY, WATCHDOG] {
            if agent_loaded(label) {
                checked(Command::new("/bin/launchctl").args([
                    "kickstart",
                    "-k",
                    &format!("{}/{label}", domain()?),
                ]))?;
            }
        }
        Ok(())
    }
    pub fn ensure_installed() -> Result<(), String> {
        install_copy()
    }
    pub fn ensure_running() -> Result<(), String> {
        install_copy()?;
        load_agent(
            PROXY,
            &[
                installed_exe().display().to_string(),
                crate::background::PROXY_FLAG.into(),
            ],
            true,
        )
    }
    pub fn enable() -> Result<(), String> {
        ensure_running()
    }
    pub fn enable_watchdog() -> Result<(), String> {
        install_copy()?;
        load_agent(
            WATCHDOG,
            &[
                installed_exe().display().to_string(),
                crate::background::WATCHDOG_FLAG.into(),
            ],
            true,
        )
    }
    pub fn disable_watchdog() {
        if let Err(e) = remove_agent(WATCHDOG) {
            eprintln!("автопатч: {e}");
        }
    }
    pub fn disable() -> Result<(), String> {
        remove_agent(PROXY)?;
        if !is_watchdog_enabled() {
            let _ = fs::remove_file(installed_exe());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_xml_escapes_paths_with_special_characters() {
        assert_eq!(
            xml("/Users/A&B/<Mac>\"'"),
            "/Users/A&amp;B/&lt;Mac&gt;&quot;&apos;"
        );
    }

    #[test]
    fn macos_patch_and_revert_keep_native_code_runnable_and_signed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ag-native-sign-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("fixture.c");
        let exe = dir.join("agy");
        fs::write(
            &source,
            b"#include <stdio.h>\nint main(void) { puts(\"ineligible https_proxy\"); return 0; }\n",
        )
        .unwrap();
        checked(
            Command::new("/usr/bin/clang")
                .arg(&source)
                .arg("-o")
                .arg(&exe),
        )
        .unwrap();
        sign_binary(&exe).unwrap();
        let summary = crate::patch_binary::patch_all_binaries(&dir);
        assert_eq!(summary.ok, 1, "{:?}", summary.last_error);
        checked(
            Command::new("/usr/bin/codesign")
                .args(["--verify", "--strict"])
                .arg(&exe),
        )
        .unwrap();
        let result = checked(&mut Command::new(&exe)).unwrap();
        assert!(String::from_utf8_lossy(&result.stdout).contains("inexigible"));
        assert_ne!(fs::metadata(&exe).unwrap().permissions().mode() & 0o111, 0);
        for (_, result) in crate::patch_binary::unpatch_all_binaries(&dir) {
            result.unwrap();
        }
        checked(
            Command::new("/usr/bin/codesign")
                .args(["--verify", "--strict"])
                .arg(&exe),
        )
        .unwrap();
        let result = checked(&mut Command::new(&exe)).unwrap();
        assert!(String::from_utf8_lossy(&result.stdout).contains("ineligible https_proxy"));
        fs::remove_dir_all(dir).unwrap();
    }
}
