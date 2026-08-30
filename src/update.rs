use std::{env, fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};

const RELEASE_LATEST: &str = "https://github.com/DreamCats/dev-cli/releases/latest";
const RELEASE_DOWNLOAD: &str = "https://github.com/DreamCats/dev-cli/releases/latest/download";

#[derive(Debug, Serialize)]
pub(crate) struct UpdateReport {
    pub(crate) current_version: String,
    pub(crate) latest_version: String,
    pub(crate) updated: bool,
}

pub(crate) fn run(check: bool) -> Result<UpdateReport> {
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let latest = latest_version()?;
    if !is_newer(&latest, &current) || check {
        return Ok(UpdateReport {
            current_version: current,
            latest_version: latest,
            updated: false,
        });
    }

    let target = target()?;
    let archive_name = format!("dev-{target}.tar.gz");
    let archive_url = format!("{RELEASE_DOWNLOAD}/{archive_name}");
    let archive_bytes = curl(&[&archive_url])
        .with_context(|| format!("the latest release has no binary for {target}"))?;
    let checksums_url = format!("{RELEASE_DOWNLOAD}/SHA256SUMS");
    let checksum_text = String::from_utf8(curl(&[&checksums_url])?)?;
    let expected = checksum_text
        .lines()
        .find_map(|line| {
            line.split_whitespace()
                .find(|part| *part == archive_name)
                .and_then(|_| line.split_whitespace().next())
        })
        .context("SHA256SUMS has no checksum for the selected archive")?;
    let actual = format!("{:x}", Sha256::digest(&archive_bytes));
    if actual != expected {
        bail!("release archive checksum verification failed");
    }

    let executable = env::current_exe().context("failed to locate the current dev binary")?;
    if executable
        .components()
        .any(|component| component.as_os_str() == "target")
    {
        bail!("refusing to replace a development binary; install a release binary first");
    }

    let temp = env::temp_dir().join(format!("dev-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp)?;
    let archive_path = temp.join(&archive_name);
    fs::write(&archive_path, archive_bytes)?;
    command(
        "tar",
        [
            "-xzf",
            archive_path.to_string_lossy().as_ref(),
            "-C",
            temp.to_string_lossy().as_ref(),
        ],
    )?;
    let replacement = temp.join("dev");
    if !replacement.is_file() {
        bail!("release archive did not contain dev");
    }
    install_replacement(&replacement, &executable)?;
    let _ = fs::remove_dir_all(temp);

    Ok(UpdateReport {
        current_version: current,
        latest_version: latest,
        updated: true,
    })
}

fn install_replacement(replacement: &Path, executable: &Path) -> Result<()> {
    let directory = executable
        .parent()
        .context("current dev binary has no parent directory")?;
    let name = executable
        .file_name()
        .context("current dev binary has no file name")?
        .to_string_lossy();
    let staged = directory.join(format!(".{name}.update-{}", std::process::id()));
    let _ = fs::remove_file(&staged);
    fs::copy(replacement, &staged)
        .with_context(|| format!("failed to stage the new dev binary at {}", staged.display()))?;
    fs::set_permissions(&staged, fs::metadata(replacement)?.permissions())?;

    let backup = executable.with_extension("old");
    let _ = fs::remove_file(&backup);
    if let Err(error) = fs::rename(executable, &backup) {
        let _ = fs::remove_file(&staged);
        return Err(error).context("failed to prepare the current binary for replacement");
    }
    if let Err(error) = fs::rename(&staged, executable) {
        let _ = fs::rename(&backup, executable);
        let _ = fs::remove_file(&staged);
        return Err(error).context("failed to install the new dev binary");
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

fn latest_version() -> Result<String> {
    let location = String::from_utf8(curl(&[
        "--output",
        "/dev/null",
        "--write-out",
        "%{url_effective}",
        RELEASE_LATEST,
    ])?)?;
    latest_version_from_url(&location)
}

fn latest_version_from_url(location: &str) -> Result<String> {
    let tag = location
        .strip_prefix("https://github.com/DreamCats/dev-cli/releases/tag/")
        .context("latest GitHub Release did not resolve to a release tag")?;
    let version = tag
        .strip_prefix('v')
        .filter(|version| !version.is_empty())
        .context("latest GitHub Release tag must start with 'v'")?;
    Ok(version.to_owned())
}

fn curl(args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("curl")
        .args(["--fail", "--location", "--silent", "--show-error"])
        .args(args)
        .output()
        .context("dev update requires curl")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if detail.is_empty() {
            bail!("curl failed while downloading release data");
        }
        bail!("curl failed while downloading release data: {detail}");
    }
    Ok(output.stdout)
}

fn command<const N: usize>(program: &str, args: [&str; N]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("dev update requires {program}"))?;
    if !status.success() {
        bail!("{program} failed while extracting the release archive");
    }
    Ok(())
}

fn target() -> Result<&'static str> {
    target_for(env::consts::OS, env::consts::ARCH)
}

fn target_for(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-musl"),
        _ => bail!("dev update does not yet publish binaries for this platform"),
    }
}

fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |version: &str| {
        version
            .split('.')
            .map(str::parse::<u64>)
            .collect::<std::result::Result<Vec<_>, _>>()
    };
    match (parse(candidate), parse(current)) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use assert_fs::TempDir;

    use super::{install_replacement, is_newer, latest_version_from_url, target_for};

    #[test]
    fn compares_release_versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
    }

    #[test]
    fn selects_static_linux_asset() {
        assert_eq!(
            target_for("linux", "x86_64").unwrap(),
            "x86_64-unknown-linux-musl"
        );
    }

    #[test]
    fn parses_latest_release_redirect() {
        assert_eq!(
            latest_version_from_url("https://github.com/DreamCats/dev-cli/releases/tag/v1.2.3")
                .unwrap(),
            "1.2.3"
        );
        assert!(latest_version_from_url("https://example.com/releases/tag/v1.2.3").is_err());
    }

    #[test]
    fn stages_the_replacement_in_the_binary_directory() {
        let source = TempDir::new().unwrap();
        let destination = TempDir::new().unwrap();
        let replacement = source.path().join("dev");
        let executable = destination.path().join("dev");
        fs::write(&replacement, "new").unwrap();
        fs::write(&executable, "old").unwrap();

        install_replacement(&replacement, &executable).unwrap();

        assert_eq!(fs::read_to_string(executable).unwrap(), "new");
        assert!(!destination.path().join("dev.old").exists());
    }
}
