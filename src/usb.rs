//! Sending designs to a USB stick for the machine. Drives come from lsblk;
//! mounting and ejecting go through udisks, like the file manager does.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
    /// Block device of the file system, like /dev/sdb1.
    pub device: String,
    /// The whole disk, for powering it off after unmounting.
    pub disk: String,
    pub label: String,
    pub size: String,
    pub mount: Option<PathBuf>,
}

impl Drive {
    pub fn title(&self) -> String {
        let name = if self.label.is_empty() { t!("USB drive") } else { &self.label };
        format!("{name} ({})", self.size)
    }
}

/// `KEY="value"` pairs from one line of `lsblk -P`.
fn pairs(line: &str) -> Vec<(&str, String)> {
    let mut out = Vec::new();
    let mut rest = line.trim();
    while let Some(eq) = rest.find("=\"") {
        let key = rest[..eq].trim();
        let after = &rest[eq + 2..];
        let Some(end) = after.find('"') else { break };
        // lsblk escapes odd characters as \xNN.
        out.push((key, unescape_hex(&after[..end])));
        rest = &after[end + 1..];
    }
    out
}

fn unescape_hex(s: &str) -> String {
    let mut bytes = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1] == b'x'
            && let Ok(v) = u8::from_str_radix(&s[i + 2..i + 4], 16)
        {
            bytes.push(v);
            i += 4;
            continue;
        }
        bytes.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// File systems on removable or USB disks, from `lsblk -P` output.
pub fn parse_lsblk(text: &str) -> Vec<Drive> {
    let rows: Vec<Vec<(&str, String)>> = text.lines().map(pairs).collect();
    let get = |row: &[(&str, String)], k: &str| row.iter().find(|(key, _)| *key == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let mut drives = Vec::new();
    for row in &rows {
        let fstype = get(row, "FSTYPE");
        if !matches!(fstype.as_str(), "vfat" | "exfat" | "ntfs" | "ntfs3") {
            continue;
        }
        let parent = get(row, "PKNAME");
        let parent_row = rows.iter().find(|r| get(r, "NAME") == parent);
        let removable = |r: &[(&str, String)]| get(r, "RM") == "1" || get(r, "HOTPLUG") == "1" || get(r, "TRAN") == "usb";
        if !(removable(row) || parent_row.is_some_and(|p| removable(p))) {
            continue;
        }
        let mount = get(row, "MOUNTPOINT");
        drives.push(Drive {
            device: get(row, "PATH"),
            disk: parent_row.map(|p| get(p, "PATH")).filter(|p| !p.is_empty()).unwrap_or_else(|| get(row, "PATH")),
            label: get(row, "LABEL"),
            size: get(row, "SIZE"),
            mount: (!mount.is_empty()).then(|| PathBuf::from(mount)),
        });
    }
    drives
}

pub fn drives() -> Vec<Drive> {
    Command::new("lsblk")
        .args(["-P", "-o", "NAME,PKNAME,PATH,MOUNTPOINT,RM,HOTPLUG,LABEL,FSTYPE,SIZE,TYPE,TRAN"])
        .output()
        .ok()
        .map(|o| parse_lsblk(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

fn udisks(args: &[&str]) -> Result<String, String> {
    let out = Command::new("udisksctl").args(args).arg("--no-user-interaction").output().map_err(|e| t!("Couldn't run udisksctl: {e}.", e = e))?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).into_owned()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// The 8.3 name the machine shows best: up to eight capitals and digits.
pub fn short_name(stem: &str) -> String {
    let name: String = stem.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).take(8).collect();
    if name.is_empty() { t!("DESIGN").into() } else { name }
}

/// Finds a folder by name regardless of case, as FAT drives often differ.
fn child_dir(parent: &Path, name: &str) -> PathBuf {
    std::fs::read_dir(parent)
        .ok()
        .and_then(|it| it.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name) && e.path().is_dir()))
        .map(|e| e.path())
        .unwrap_or_else(|| parent.join(name))
}

/// Mounts the drive if needed and writes the design into `folder` (path
/// parts, matched without regard to case; empty for the top of the stick),
/// where the machine looks for embroidery. Returns the written file.
pub fn send(drive: &Drive, folder: &[&str], file_name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let root = match &drive.mount {
        Some(m) => m.clone(),
        None => {
            let out = udisks(&["mount", "-b", &drive.device]).map_err(|e| t!("Couldn't open the USB drive: {e}", e = e))?;
            // "Mounted /dev/sdb1 at /run/media/user/LABEL"
            out.trim().rsplit_once(" at ").map(|(_, p)| PathBuf::from(p.trim_end_matches('.'))).ok_or(t!("The USB drive didn't say where it was opened."))?
        }
    };
    let folder = folder.iter().fold(root, |dir, name| child_dir(&dir, name));
    std::fs::create_dir_all(&folder).map_err(|e| t!("Couldn't make the {folder} folder: {e}.", folder = folder.display(), e = e))?;
    let target = folder.join(file_name);
    let tmp = folder.join(format!("{file_name}.tmp"));
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        // Make sure it is really on the stick before anyone pulls it out.
        f.sync_all()?;
        std::fs::rename(&tmp, &target)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        t!("Couldn't write to the USB drive: {e}.", e = e)
    })?;
    Ok(target)
}

/// Unmounts the drive and powers it off so it can be pulled out.
pub fn eject(drive: &Drive) -> Result<(), String> {
    let _ = Command::new("sync").status();
    if drives().iter().any(|d| d.device == drive.device && d.mount.is_some()) {
        udisks(&["unmount", "-b", &drive.device]).map_err(|e| t!("Couldn't close the USB drive: {e}", e = e))?;
    }
    // Powering off is a nicety; some readers don't support it.
    let _ = udisks(&["power-off", "-b", &drive.disk]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_usb_file_systems_only() {
        let text = r#"NAME="nvme0n1p1" PKNAME="nvme0n1" PATH="/dev/nvme0n1p1" MOUNTPOINT="/boot" RM="0" HOTPLUG="0" LABEL="" FSTYPE="vfat" SIZE="2G" TYPE="part" TRAN=""
NAME="nvme0n1" PKNAME="" PATH="/dev/nvme0n1" MOUNTPOINT="" RM="0" HOTPLUG="0" LABEL="" FSTYPE="" SIZE="476.9G" TYPE="disk" TRAN="nvme"
NAME="sdb" PKNAME="" PATH="/dev/sdb" MOUNTPOINT="" RM="1" HOTPLUG="1" LABEL="" FSTYPE="" SIZE="7.5G" TYPE="disk" TRAN="usb"
NAME="sdb1" PKNAME="sdb" PATH="/dev/sdb1" MOUNTPOINT="/run/media/cody/JANOME\x20USB" RM="1" HOTPLUG="1" LABEL="JANOME\x20USB" FSTYPE="vfat" SIZE="7.5G" TYPE="part" TRAN="""#;
        let d = parse_lsblk(text);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].device, "/dev/sdb1");
        assert_eq!(d[0].disk, "/dev/sdb");
        assert_eq!(d[0].label, "JANOME USB");
        assert_eq!(d[0].mount.as_deref(), Some(Path::new("/run/media/cody/JANOME USB")));
    }

    #[test]
    fn short_names_fit_the_machine() {
        assert_eq!(short_name("GEAR Bremen logo"), "GEARBREM");
        assert_eq!(short_name("Hello"), "HELLO");
        assert_eq!(short_name("—"), "DESIGN");
    }

    #[test]
    fn writes_into_existing_emb_folder_whatever_its_case() {
        let root = std::env::temp_dir().join(format!("hoop-usb-{}", std::process::id()));
        std::fs::create_dir_all(root.join("emb/EMBF")).unwrap();
        let drive = Drive { device: "/dev/x".into(), disk: "/dev/x".into(), label: "X".into(), size: "1G".into(), mount: Some(root.clone()) };
        let written = send(&drive, &["EMB", "Embf"], "HELLO.jef", b"data").unwrap();
        assert_eq!(written, root.join("emb/EMBF/HELLO.jef"));
        assert_eq!(std::fs::read(&written).unwrap(), b"data");
        std::fs::remove_dir_all(root).unwrap();
    }
}
