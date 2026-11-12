//! Self-update from GitHub releases (`yate update`) and a once-a-day check
//! for new versions while the editor runs.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_URL: &str = "https://github.com/sobanieca/yate/releases";
pub const NO_UPDATE_CHECK_ENV: &str = "YATE_NO_UPDATE_CHECK";
const RELEASES_URL_ENV: &str = "YATE_RELEASES_URL";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

fn releases_url() -> String {
    std::env::var(RELEASES_URL_ENV)
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| RELEASES_URL.to_string())
}

fn asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("yate-linux-x64"),
        ("linux", "aarch64") => Some("yate-linux-arm64"),
        ("macos", "x86_64") => Some("yate-macos-x64"),
        ("macos", "aarch64") => Some("yate-macos-arm64"),
        _ => None,
    }
}

fn agent(timeout: Option<Duration>, max_redirects: u32) -> ureq::Agent {
    ureq::Agent::config_builder()
        .max_redirects(max_redirects)
        .timeout_global(timeout)
        .user_agent(format!("yate/{VERSION}"))
        .build()
        .new_agent()
}

/// Reads the latest version from the redirect of `/releases/latest` to
/// `/releases/tag/v<version>`, which has no GitHub API rate limits.
fn latest_version(timeout: Option<Duration>) -> Result<String, String> {
    let url = format!("{}/latest", releases_url());
    let response = agent(timeout, 0)
        .get(&url)
        .call()
        .map_err(|err| format!("Failed to check the latest version: {err}"))?;
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "Failed to check the latest version: no release found".to_string())?;
    version_from_location(location).ok_or_else(|| {
        format!("Failed to check the latest version: unexpected release URL '{location}'")
    })
}

fn version_from_location(location: &str) -> Option<String> {
    let tag = location.trim_end_matches('/').rsplit('/').next()?;
    let version = tag.strip_prefix('v').unwrap_or(tag);
    parse_version(version).map(|_| version.to_string())
}

fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    }
}

fn map_io(err: std::io::Error, exe: &Path) -> String {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        format!(
            "Permission denied while replacing {}. Re-run with elevated permissions, e.g.: sudo yate update",
            exe.display()
        )
    } else {
        format!("Failed to replace {}: {err}", exe.display())
    }
}

/// `yate update`: replaces the running binary with the latest release.
pub fn execute() -> Result<(), String> {
    println!("Checking for updates...");
    let latest = latest_version(Some(Duration::from_secs(30)))?;
    write_cache(&latest);
    if !is_newer(&latest, VERSION) {
        println!("yate is up to date ({VERSION})");
        return Ok(());
    }

    let asset = asset_name().ok_or_else(|| {
        format!(
            "Automatic updates are not available for {}-{}. Download yate manually from {RELEASES_URL}/latest",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    })?;
    let exe = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|err| format!("Failed to locate the yate binary: {err}"))?;

    println!("Updating yate {VERSION} -> {latest}...");
    let url = format!("{}/download/v{latest}/{asset}.gz", releases_url());
    let mut response = agent(Some(Duration::from_secs(300)), 10)
        .get(&url)
        .call()
        .map_err(|err| format!("Failed to download {url}: {err}"))?;
    let compressed = response
        .body_mut()
        .with_config()
        .limit(u64::MAX)
        .read_to_vec()
        .map_err(|err| format!("Failed to download {url}: {err}"))?;
    let binary = gunzip(&compressed).map_err(|err| format!("Failed to unpack {url}: {err}"))?;

    let temp = exe.with_file_name(format!(".yate-update-{}", std::process::id()));
    let result = install(&temp, &exe, &binary, &latest);
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;

    println!("yate has been updated to {latest}");
    Ok(())
}

fn gunzip(compressed: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut binary = Vec::new();
    flate2::read::GzDecoder::new(compressed).read_to_end(&mut binary)?;
    Ok(binary)
}

fn install(temp: &Path, exe: &Path, binary: &[u8], version: &str) -> Result<(), String> {
    std::fs::write(temp, binary).map_err(|err| map_io(err, exe))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(temp, std::fs::Permissions::from_mode(0o755))
            .map_err(|err| map_io(err, exe))?;
    }

    let output = std::process::Command::new(temp)
        .arg("--version")
        .env(NO_UPDATE_CHECK_ENV, "1")
        .output()
        .map_err(|err| format!("Downloaded binary could not be started: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = stdout.trim();
    let reported = reported.strip_prefix("yate ").unwrap_or(reported);
    if reported != version {
        return Err(format!(
            "Downloaded binary reports version '{reported}' instead of '{version}', update aborted"
        ));
    }

    std::fs::rename(temp, exe).map_err(|err| map_io(err, exe))
}

fn cache_file() -> Option<PathBuf> {
    Some(crate::config::dir().join("latest-version"))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_cache() -> Option<(u64, String)> {
    let content = std::fs::read_to_string(cache_file()?).ok()?;
    let mut lines = content.lines();
    let checked_at = lines.next()?.parse().ok()?;
    Some((checked_at, lines.next().unwrap_or_default().to_string()))
}

fn write_cache(version: &str) {
    let Some(path) = cache_file() else { return };
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let temp = dir.join(format!(".latest-version-{}", std::process::id()));
    if std::fs::write(&temp, format!("{}\n{version}\n", now())).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

pub fn check_disabled_by_env() -> bool {
    std::env::var_os(NO_UPDATE_CHECK_ENV).is_some_and(|value| !value.is_empty())
}

/// The latest version when it is newer than this one: from the cache when
/// it was checked in the last day, otherwise from GitHub. Blocks; run it on
/// a background thread.
pub fn newer_version() -> Option<String> {
    let latest = match read_cache() {
        Some((checked_at, latest))
            if now().saturating_sub(checked_at) < CHECK_INTERVAL.as_secs() =>
        {
            Some(latest)
        }
        _ => {
            let latest = latest_version(Some(Duration::from_secs(3))).ok();
            write_cache(latest.as_deref().unwrap_or_default());
            latest
        }
    };
    latest.filter(|latest| is_newer(latest, VERSION))
}

pub fn available_message(latest: &str) -> String {
    format!("yate {latest} is available: run 'yate update' to install it")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_gzipped_release_assets() {
        use std::io::Write;
        let binary = b"\x7fELF yate binary".repeat(1000);
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&binary).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() < binary.len());
        assert_eq!(gunzip(&compressed).unwrap(), binary);
        assert!(gunzip(b"not gzip").is_err());
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("10.0.0", "9.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
        assert_eq!(parse_version("1.0.0-beta.1"), Some((1, 0, 0)));
    }

    #[test]
    fn reads_the_version_from_the_release_url() {
        assert_eq!(
            version_from_location("https://github.com/sobanieca/yate/releases/tag/v1.2.3"),
            Some("1.2.3".to_string())
        );
        assert_eq!(
            version_from_location("https://github.com/sobanieca/yate/releases"),
            None
        );
    }
}
