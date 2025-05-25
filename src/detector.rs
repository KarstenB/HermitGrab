// Built-in detectors for HermitGrab
// Detects: hostname, architecture (docker style), OS, OS version (numeric), OS version nickname

use std::collections::HashSet;

pub fn detect_builtin_tags() -> HashSet<String> {
    let mut tags = HashSet::new();
    // Hostname
    if let Ok(hostname) = get_hostname() {
        tags.insert(hostname);
    }
    // Architecture (docker style)
    if let Some(arch) = get_architecture() {
        tags.insert(arch);
    }
    // OS and version
    if let Some((os, version, nickname)) = get_os_info() {
        tags.insert(os);
        if let Some(ver) = version {
            tags.insert(ver);
        }
        if let Some(nick) = nickname {
            tags.insert(nick);
        }
    }
    tags
}

fn get_hostname() -> Result<String, std::io::Error> {
    hostname::get().map(|h| h.to_string_lossy().to_string())
}

fn get_architecture() -> Option<String> {
    // Use docker style: amd64, arm64, etc.
    match std::env::consts::ARCH {
        "x86_64" => Some("amd64".to_string()),
        "aarch64" => Some("arm64".to_string()),
        other => Some(other.to_string()),
    }
}

fn get_os_info() -> Option<(String, Option<String>, Option<String>)> {
    // Returns (os, version, nickname)
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let os = "macos".to_string();
        let sw_vers = Command::new("sw_vers").output().ok()?;
        let out = String::from_utf8_lossy(&sw_vers.stdout);
        let mut version = None;
        let mut nickname = None;
        for line in out.lines() {
            if line.starts_with("ProductVersion:") {
                version = Some(line.split(':').nth(1)?.trim().to_string());
            }
            if line.starts_with("ProductName:") {
                nickname = Some(line.split(':').nth(1)?.trim().to_string());
            }
        }
        Some((os, version, nickname))
    }
    #[cfg(target_os = "linux")]
    {
        // Try /etc/os-release
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            let mut os = "linux".to_string();
            let mut version = None;
            let mut nickname = None;
            for line in content.lines() {
                if line.starts_with("ID=") {
                    os = line[3..].trim_matches('"').to_string();
                }
                if line.starts_with("VERSION_ID=") {
                    version = Some(line[11..].trim_matches('"').to_string());
                }
                if line.starts_with("VERSION_CODENAME=") {
                    nickname = Some(line[17..].trim_matches('"').to_string());
                }
            }
            return Some((os, version, nickname));
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        let os = "windows".to_string();
        // Version and nickname detection for Windows can be added here
        Some((os, None, None))
    }
}
