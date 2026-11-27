//! Archive extraction is a prepare operation: sources are never deleted here.
//! The caller must publish and durably record the returned inventory before cleanup.
#[cfg(windows)]
#[path = "archive_process.rs"]
mod process;
use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_LISTING_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 500_000;
const BUFFER_SIZE: usize = 128 * 1024;
const WORKSPACE_MARKER: &str = ".nna-archive-owner";
const WORKSPACE_OWNER: &str = "NNA-ARCHIVE-WORKSPACE-V1\n";
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = (value >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(value & 1)));
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub is_directory: bool,
    pub crc32: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub hash: String,
    #[serde(default)]
    pub is_directory: bool,
}

#[derive(Debug)]
pub struct ArchiveExtraction {
    pub staging_path: PathBuf,
    pub inventory: Vec<VerifiedEntry>,
    pub source_paths: Vec<PathBuf>,
    // Prepared content remains helper-owned until the caller moves it into
    // durable storage. Any publication/verification failure cleans staging.
    _workspace: WorkspaceGuard,
}

/// How far an extraction is, for another thread to show while it runs: 7-Zip's own percent
/// is the first half, and checking what it wrote the second.
#[derive(Debug, Default)]
pub struct ExtractionProgress {
    written: AtomicU64,
    checked: AtomicU64,
    size: AtomicU64,
}

impl ExtractionProgress {
    pub fn percent(&self) -> u64 {
        let size = self.size.load(Ordering::Relaxed);
        let checked = match size {
            0 => 0,
            size => {
                let checked = self.checked.load(Ordering::Relaxed).min(size);
                (u128::from(checked) * 100 / u128::from(size)) as u64
            }
        };
        (self.written.load(Ordering::Relaxed).min(100) + checked) / 2
    }
}

#[derive(Debug)]
struct WorkspaceGuard {
    root: PathBuf,
    parent: PathBuf,
    armed: bool,
    owner: Option<File>,
}

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        // This exact, fresh UUID child was created by this invocation. Never
        // derive a deletion target from archive member names or caller records.
        if self.armed && self.root.parent() == Some(self.parent.as_path()) {
            self.owner.take();
            let _ = remove_workspace(&self.root);
        }
    }
}

/// Recover only workspaces durably marked by this helper whose lifetime lock
/// has been released. Legacy, foreign, linked and active children remain untouched.
pub(super) fn recover_workspaces(staging_parent: &Path) -> Result<usize> {
    if !staging_parent.is_dir() || validate_no_links(staging_parent).is_err() {
        return Ok(0);
    }
    let parent = staging_parent
        .canonicalize()
        .context("storage_staging_cleanup_failed")?;
    let mut removed = 0;
    for child in fs::read_dir(&parent).context("storage_staging_cleanup_failed")? {
        let Ok(child) = child else {
            continue;
        };
        let name = child.file_name();
        let Some(identity) = name.to_str().and_then(|name| name.strip_prefix("archive-")) else {
            continue;
        };
        let Ok(id) = uuid::Uuid::parse_str(identity) else {
            continue;
        };
        if id.to_string() != identity {
            continue;
        }
        let workspace = child.path();
        if workspace.parent() != Some(parent.as_path())
            || validate_no_links(&workspace).is_err()
            || !workspace.is_dir()
        {
            continue;
        }
        let marker = workspace.join(WORKSPACE_MARKER);
        if validate_no_links(&marker).is_err() {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&marker) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > 128 {
            continue;
        }
        let Ok(mut owner) = OpenOptions::new().read(true).write(true).open(&marker) else {
            continue;
        };
        if fs2::FileExt::try_lock_exclusive(&owner).is_err() {
            continue;
        }
        let mut body = Vec::new();
        if (&mut owner).take(129).read_to_end(&mut body).is_err()
            || body != format!("{WORKSPACE_OWNER}{id}\n").as_bytes()
        {
            continue;
        }
        // The UUID child, matching durable owner marker and released OS lock
        // prove ownership before this exact bounded deletion is attempted.
        drop(owner);
        if remove_workspace(&workspace).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn remove_workspace(workspace: &Path) -> Result<()> {
    validate_no_links(workspace)?;
    for child in fs::read_dir(workspace).context("storage_staging_cleanup_failed")? {
        let child = child.context("storage_staging_cleanup_failed")?;
        if child.file_name() != WORKSPACE_MARKER {
            remove_private_tree(&child.path())?;
        }
    }
    // Retain ownership proof until every payload child has been removed, so a
    // locked file can be retried at the next startup without guessing ownership.
    let marker = workspace.join(WORKSPACE_MARKER);
    if marker
        .try_exists()
        .context("storage_staging_cleanup_failed")?
    {
        fs::remove_file(marker).context("storage_staging_cleanup_failed")?;
    }
    fs::remove_dir(workspace).context("storage_staging_cleanup_failed")?;
    Ok(())
}

/// Remove only a helper-owned workspace. Reparse entries are unlinked, never
/// descended into, even after a failed or malicious extraction.
fn remove_private_tree(path: &Path) -> Result<()> {
    let mut pending = vec![(path.to_owned(), false)];
    while let Some((path, visited)) = pending.pop() {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => bail!("storage_staging_cleanup_failed"),
        };
        #[allow(unused_mut)]
        let mut reparse = metadata.file_type().is_symlink();
        #[allow(unused_mut)]
        let mut directory = metadata.is_dir();
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            reparse |= metadata.file_attributes() & 0x400 != 0;
            directory |= metadata.file_attributes() & 0x10 != 0;
        }
        if reparse {
            if directory {
                fs::remove_dir(&path)
            } else {
                fs::remove_file(&path)
            }
            .context("storage_staging_cleanup_failed")?;
            continue;
        }
        #[cfg(windows)]
        if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(false);
            fs::set_permissions(&path, permissions).context("storage_staging_cleanup_failed")?;
        }
        if directory && !visited {
            pending.push((path.clone(), true));
            for item in fs::read_dir(&path).context("storage_staging_cleanup_failed")? {
                pending.push((
                    item.context("storage_staging_cleanup_failed")?.path(),
                    false,
                ));
            }
        } else if directory {
            fs::remove_dir(&path).context("storage_staging_cleanup_failed")?;
        } else {
            fs::remove_file(&path).context("storage_staging_cleanup_failed")?;
        }
    }
    Ok(())
}

fn check_cancel(cancelled: &AtomicBool) -> Result<()> {
    ensure!(!cancelled.load(Ordering::Relaxed), "cancelled");
    Ok(())
}

/// Portable path policy, including aliases that Win32 would silently normalize.
/// Backslashes are archive separators even when this runs on Unix.
pub fn validate_relative_path(value: &str) -> Result<PathBuf> {
    ensure!(
        !value.is_empty() && value.len() <= 32_000,
        "archive_unsafe_path"
    );
    ensure!(!value.starts_with(['/', '\\']), "archive_unsafe_path");
    let normalized = value.replace('\\', "/");
    let mut result = PathBuf::new();
    for component in normalized.split('/') {
        ensure!(
            !component.is_empty() && component != "." && component != "..",
            "archive_unsafe_path"
        );
        ensure!(!component.ends_with(['.', ' ']), "archive_unsafe_path");
        ensure!(
            !component
                .chars()
                .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*')),
            "archive_unsafe_path"
        );
        ensure!(!reserved_device(component), "archive_unsafe_path");
        result.push(component);
    }
    Ok(result)
}

/// Win32 device names, with or without an extension (CON, con.txt, COM1 .zip).
pub(super) fn reserved_device(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(
            &stem[3..],
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
        ))
}

fn portable_path(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .context("archive_invalid_unicode")?
        .replace('\\', "/"))
}

/// Reject symlinks/reparse points at the target and every existing ancestor.
pub fn validate_no_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                ensure!(!metadata.file_type().is_symlink(), "storage_link_forbidden");
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        metadata.file_attributes() & 0x400 == 0,
                        "storage_link_forbidden"
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(anyhow::Error::new(error).context("storage_path_unavailable")),
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum VolumeKind {
    Single,
    Numbered,
    PartRar,
    ClassicRar,
    SplitZip,
}

#[derive(Clone, Debug)]
struct VolumeName {
    key: String,
    kind: VolumeKind,
    index: u32,
}

fn volume_name(name: &str) -> Option<VolumeName> {
    let lower = name.to_lowercase();
    if lower.ends_with(".unitypackage") {
        return None;
    }
    if let Some((base, suffix)) = lower.rsplit_once('.') {
        if (base.ends_with(".zip") || base.ends_with(".7z"))
            && suffix.len() >= 3
            && suffix.bytes().all(|v| v.is_ascii_digit())
        {
            return Some(VolumeName {
                key: base.to_owned(),
                kind: VolumeKind::Numbered,
                index: suffix.parse().ok()?,
            });
        }
        if suffix.len() == 3 && suffix.as_bytes()[1..].iter().all(u8::is_ascii_digit) {
            let number: u32 = suffix[1..].parse().ok()?;
            if suffix.starts_with('r') {
                return Some(VolumeName {
                    key: base.to_owned(),
                    kind: VolumeKind::ClassicRar,
                    index: number + 1,
                });
            }
            if suffix.starts_with('z') {
                return Some(VolumeName {
                    key: base.to_owned(),
                    kind: VolumeKind::SplitZip,
                    index: number,
                });
            }
        }
    }
    if let Some(base) = lower.strip_suffix(".rar") {
        if let Some((prefix, part)) = base.rsplit_once(".part") {
            if !part.is_empty() && part.bytes().all(|v| v.is_ascii_digit()) {
                return Some(VolumeName {
                    key: prefix.to_owned(),
                    kind: VolumeKind::PartRar,
                    index: part.parse().ok()?,
                });
            }
        }
        return Some(VolumeName {
            key: base.to_owned(),
            kind: VolumeKind::ClassicRar,
            index: 0,
        });
    }
    if let Some(base) = lower.strip_suffix(".zip") {
        return Some(VolumeName {
            key: base.to_owned(),
            kind: VolumeKind::SplitZip,
            index: 0,
        });
    }
    if lower.ends_with(".7z") {
        Some(VolumeName {
            key: lower,
            kind: VolumeKind::Single,
            index: 0,
        })
    } else {
        None
    }
}

pub fn is_supported_archive(path: &Path) -> bool {
    path.file_name()
        .and_then(|v| v.to_str())
        .and_then(volume_name)
        .is_some()
}

/// Returns the first volume first, followed by the remaining source paths.
/// A missing final volume is additionally detected by listing/extraction.
pub fn validate_volume_group(archive: &Path, available: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let name = archive
        .file_name()
        .and_then(|v| v.to_str())
        .context("archive_invalid_name")?;
    let wanted = volume_name(name).context("archive_unsupported")?;
    let mut members = BTreeMap::new();
    for candidate in std::iter::once(archive).chain(available.iter().map(PathBuf::as_path)) {
        let Some(candidate_name) = candidate.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        let Some(volume) = volume_name(candidate_name) else {
            continue;
        };
        if volume.key != wanted.key || volume.kind != wanted.kind {
            continue;
        }
        validate_relative_path(candidate_name)?;
        if let Some(existing) = members.get(&volume.index) {
            ensure!(existing == candidate, "archive_volume_collision");
        } else {
            members.insert(volume.index, candidate.to_owned());
        }
    }
    let first = if matches!(wanted.kind, VolumeKind::Numbered | VolumeKind::PartRar) {
        1
    } else {
        0
    };
    ensure!(
        members.keys().next().copied() == Some(first),
        "archive_missing_volume"
    );
    let mut expected = first;
    for index in members.keys() {
        ensure!(*index == expected, "archive_missing_volume");
        expected = expected.checked_add(1).context("archive_volume_limit")?;
    }
    Ok(members.into_values().collect())
}

/// Parse only the member section after 7-Zip's technical-list separator. An encrypted
/// member needs `password`.
pub fn parse_listing(listing: &str, password: bool) -> Result<Vec<ArchiveEntry>> {
    let normalized = listing.replace("\r\n", "\n");
    let (_, members) = normalized
        .split_once("\n----------\n")
        .context("archive_listing_invalid")?;
    let mut entries = Vec::new();
    for block in members
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
    {
        // Successful technical listings can append this summary after the
        // member records (for example, a ZIP with harmless trailing bytes).
        if let Some(count) = block.trim().strip_prefix("Warnings: ") {
            ensure!(count.parse::<u64>().is_ok(), "archive_listing_invalid");
            continue;
        }
        let mut fields = BTreeMap::new();
        for line in block.lines() {
            let (key, value) = line.split_once(" = ").context("archive_listing_invalid")?;
            ensure!(
                fields.insert(key, value).is_none(),
                "archive_listing_ambiguous"
            );
        }
        let raw_path = fields.get("Path").context("archive_listing_invalid")?;
        let path = portable_path(&validate_relative_path(raw_path)?)?;
        for key in [
            "Symbolic Link",
            "Hard Link",
            "Link",
            "Copy Link",
            "Reparse",
            "Reparse Point",
        ] {
            ensure!(
                fields.get(key).is_none_or(|v| v.is_empty() || *v == "-"),
                "archive_link_forbidden"
            );
        }
        ensure!(
            password || fields.get("Encrypted") != Some(&"+"),
            "archive_password_required"
        );
        for key in ["Alternate Stream", "Anti", "Deleted"] {
            ensure!(
                fields.get(key).is_none_or(|v| v.is_empty() || *v == "-"),
                "archive_unsupported_member"
            );
        }
        if let Some(links) = fields.get("Links") {
            ensure!(
                links.is_empty() || links.parse::<u64>().is_ok_and(|v| v <= 1),
                "archive_link_forbidden"
            );
        }
        let attributes = fields.get("Attributes").copied().unwrap_or("");
        let mode = fields.get("Mode").copied().unwrap_or("");
        for token in attributes.split_whitespace().chain(mode.split_whitespace()) {
            // Unix file modes expose symlink/device/socket/fifo types. Windows L is reparse.
            ensure!(
                !token.starts_with(['l', 'b', 'c', 's', 'p']) && !token.contains('L'),
                "archive_link_forbidden"
            );
        }
        let is_directory = fields.get("Folder") == Some(&"+")
            || attributes
                .split_whitespace()
                .any(|v| v.starts_with('D') || v.starts_with('d'))
            || mode.starts_with('d');
        let size_bytes = match fields.get("Size") {
            Some(value) if !value.is_empty() => value.parse().context("archive_listing_invalid")?,
            _ if is_directory => 0,
            _ => bail!("archive_listing_invalid"),
        };
        ensure!(!is_directory || size_bytes == 0, "archive_listing_invalid");
        let crc32 = fields
            .get("CRC")
            .filter(|v| !v.is_empty())
            .map(|v| u32::from_str_radix(v, 16).context("archive_listing_invalid"))
            .transpose()?;
        entries.push(ArchiveEntry {
            relative_path: path,
            size_bytes,
            is_directory,
            crc32,
        });
        ensure!(entries.len() <= MAX_ENTRIES, "archive_entry_limit");
    }
    ensure!(!entries.is_empty(), "archive_empty");
    Ok(entries)
}

/// Windows compares names by simple uppercase; a character whose uppercase form is longer
/// (such as ß) compares as itself.
fn fold_case(path: &str) -> String {
    path.chars()
        .map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) => single,
                _ => c,
            }
        })
        .collect()
}

/// Where each listed member lands in the staging tree.
struct ExtractionPlan {
    /// Each explicit folder once, then every member file at its own path.
    outputs: Vec<ArchiveEntry>,
    /// (written copy, kept file): same size and CRC as a file already at that path.
    identical: Vec<(String, String)>,
}

/// Folders that differ only by letter case merge into the first-seen spelling, as Windows
/// does. Folders are created before extraction, so a file whose path is taken by a folder
/// or an earlier file is kept too: 7-Zip's -aou writes it as `name_1.ext`, `name_2.ext`…
/// and never overwrites. This predicts those names so the result can be verified exactly.
fn plan_extraction(entries: &[ArchiveEntry]) -> ExtractionPlan {
    let mut folders: BTreeMap<String, String> = BTreeMap::new();
    let mut spell = |path: &str| {
        let mut spelling = String::new();
        for part in path.split('/') {
            let next = if spelling.is_empty() {
                part.to_owned()
            } else {
                format!("{spelling}/{part}")
            };
            spelling = folders.entry(fold_case(&next)).or_insert(next).clone();
        }
        spelling
    };
    let mut outputs: Vec<ArchiveEntry> = Vec::new();
    let mut explicit = BTreeSet::new();
    let mut files = Vec::new();
    for entry in entries {
        if entry.is_directory {
            let path = spell(&entry.relative_path);
            if explicit.insert(fold_case(&path)) {
                outputs.push(ArchiveEntry {
                    relative_path: path,
                    ..entry.clone()
                });
            }
        } else {
            let path = match entry.relative_path.rsplit_once('/') {
                Some((parent, name)) => format!("{}/{name}", spell(parent)),
                None => entry.relative_path.clone(),
            };
            files.push((path, entry));
        }
    }
    let mut taken: BTreeSet<String> = folders.into_keys().collect();
    // Files that wanted a path, by that path, for recognizing identical copies.
    let mut wanted: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut identical = Vec::new();
    for (path, entry) in files {
        let key = fold_case(&path);
        let (target, copy_of) = if taken.contains(&key) {
            let copy_of = wanted.get(&key).and_then(|placed| {
                placed.iter().copied().find(|&index| {
                    entry.crc32.is_some()
                        && outputs[index].crc32 == entry.crc32
                        && outputs[index].size_bytes == entry.size_bytes
                })
            });
            (auto_rename(&path, &taken), copy_of)
        } else {
            (path, None)
        };
        taken.insert(fold_case(&target));
        match copy_of {
            Some(kept) => identical.push((target.clone(), outputs[kept].relative_path.clone())),
            None => wanted.entry(key).or_default().push(outputs.len()),
        }
        outputs.push(ArchiveEntry {
            relative_path: target,
            ..entry.clone()
        });
    }
    ExtractionPlan { outputs, identical }
}

/// 7-Zip's -aou name: `name_N.ext` (the last dot not leading the name starts the
/// extension), with N from its binary search over existing names.
fn auto_rename(path: &str, taken: &BTreeSet<String>) -> String {
    let (parent, name) = match path.rsplit_once('/') {
        Some((parent, name)) => (format!("{parent}/"), name),
        None => (String::new(), path),
    };
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name, ""),
    };
    let candidate = |n: u32| format!("{parent}{stem}_{n}{extension}");
    let (mut low, mut high) = (1u32, 1u32 << 30);
    while low != high {
        let middle = (low + high) / 2;
        if taken.contains(&fold_case(&candidate(middle))) {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    candidate(high)
}

fn validate_inventory_paths(entries: &[ArchiveEntry]) -> Result<BTreeMap<String, bool>> {
    let mut explicit = BTreeSet::new();
    let mut paths: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for entry in entries {
        let path = portable_path(&validate_relative_path(&entry.relative_path)?)?;
        ensure!(explicit.insert(fold_case(&path)), "archive_path_collision");
        let components: Vec<_> = path.split('/').collect();
        for length in 1..=components.len() {
            let spelling = components[..length].join("/");
            let key = fold_case(&spelling);
            let directory = length < components.len() || entry.is_directory;
            if let Some((old_spelling, old_directory)) = paths.get(&key) {
                ensure!(
                    *old_spelling == spelling && *old_directory == directory,
                    "archive_path_collision"
                );
            } else {
                paths.insert(key, (spelling, directory));
            }
        }
    }
    Ok(paths.into_values().collect())
}

fn read_bounded(mut reader: impl Read, limit: usize, overflow: Arc<AtomicBool>) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer).context("archive_process_io")?;
        if count == 0 {
            break;
        }
        if result.len().saturating_add(count) > limit {
            overflow.store(true, Ordering::Relaxed);
        } else {
            result.extend_from_slice(&buffer[..count]);
        }
    }
    Ok(result)
}

/// Reads 7-Zip's output while it extracts. A progress line (`" 45% 3 - name"` between
/// carriage returns) only sets `percent`, and the blank ones that clear it are dropped, so a
/// long extraction cannot fill what is kept for classifying an error.
fn read_extraction_output(
    mut reader: impl Read,
    limit: usize,
    overflow: Arc<AtomicBool>,
    percent: &AtomicU64,
) -> Result<Vec<u8>> {
    let mut kept = Vec::new();
    let mut line = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader.read(&mut buffer).context("archive_process_io")?;
        for &byte in &buffer[..count] {
            line.push(byte);
            if matches!(byte, b'\r' | b'\n') {
                keep_output_line(&mut kept, &mut line, limit, &overflow, percent);
            } else if line.len() > limit {
                overflow.store(true, Ordering::Relaxed);
                line.clear();
            }
        }
        if count == 0 {
            keep_output_line(&mut kept, &mut line, limit, &overflow, percent);
            return Ok(kept);
        }
    }
}

fn keep_output_line(
    kept: &mut Vec<u8>,
    line: &mut Vec<u8>,
    limit: usize,
    overflow: &AtomicBool,
    percent: &AtomicU64,
) {
    let text = line.trim_ascii();
    let digits = text.iter().take_while(|byte| byte.is_ascii_digit()).count();
    match text.get(digits) {
        Some(b'%') if digits > 0 => {
            if let Some(value) = std::str::from_utf8(&text[..digits])
                .ok()
                .and_then(|digits| digits.parse::<u64>().ok())
            {
                percent.store(value.min(100), Ordering::Relaxed);
            }
        }
        // MiB done, which 7-Zip prints when it knows no total.
        Some(b'M') if digits > 0 => {}
        _ if text.is_empty() => {}
        _ if kept.len() + line.len() > limit => overflow.store(true, Ordering::Relaxed),
        _ => kept.extend_from_slice(line),
    }
    line.clear();
}

/// Runs 7-Zip with `args`. A `password` reaches it on its input, never on the command line:
/// 7-Zip's own parser cannot carry every character there, and other programs can read it.
fn run_7zip(
    executable: &Path,
    args: &[OsString],
    cancelled: &AtomicBool,
    listing: bool,
    progress: Option<&ExtractionProgress>,
    password: Option<&str>,
) -> Result<Vec<u8>> {
    check_cancel(cancelled)?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(if password.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    let child_job = process::ChildJob::new()?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
    }
    let mut child = command
        .spawn()
        .map_err(|_| anyhow::anyhow!("archive_tool_unavailable"))?;
    #[cfg(windows)]
    if let Err(error) = child_job.attach_and_resume(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    if let (Some(password), Some(mut input)) = (password, child.stdin.take()) {
        // 7-Zip reads it as one line when it first needs it, or never. The short line waits
        // in the pipe either way; closing the input ends any further question.
        let _ = input.write_all(format!("{password}\n").as_bytes());
    }
    let overflow = Arc::new(AtomicBool::new(false));
    let out_pipe = child.stdout.take().context("archive_process_io")?;
    let err_pipe = child.stderr.take().context("archive_process_io")?;
    let out_overflow = overflow.clone();
    let err_overflow = overflow.clone();
    let written = Arc::new(AtomicU64::new(0));
    let out_written = written.clone();
    let out = thread::spawn(move || {
        if listing {
            read_bounded(out_pipe, MAX_LISTING_BYTES, out_overflow)
        } else {
            read_extraction_output(out_pipe, 1024 * 1024, out_overflow, &out_written)
        }
    });
    let err = thread::spawn(move || read_bounded(err_pipe, 1024 * 1024, err_overflow));
    let mut interrupted = None;
    let started = Instant::now();
    let timeout = if listing {
        Duration::from_secs(10 * 60)
    } else {
        Duration::from_secs(60 * 60)
    };
    let status = loop {
        if let Some(progress) = progress {
            progress
                .written
                .store(written.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        if cancelled.load(Ordering::Relaxed)
            || overflow.load(Ordering::Relaxed)
            || started.elapsed() >= timeout
        {
            interrupted = Some(if cancelled.load(Ordering::Relaxed) {
                "cancelled"
            } else if overflow.load(Ordering::Relaxed) {
                "archive_output_limit"
            } else {
                "archive_timeout"
            });
            let _ = child.kill();
            break child.wait();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error);
            }
        }
    };
    let output = out
        .join()
        .map_err(|_| anyhow::anyhow!("archive_process_io"))??;
    let stderr = err
        .join()
        .map_err(|_| anyhow::anyhow!("archive_process_io"))??;
    if let Some(code) = interrupted {
        bail!(code);
    }
    ensure!(!overflow.load(Ordering::Relaxed), "archive_output_limit");
    // Only fixed classifications leave this boundary, never private CLI output.
    let status = status.context("archive_process_io")?;
    if !status.success() {
        let code = classify_process_error(status.code(), &output, &stderr);
        // Asked for a password it was given, 7-Zip found that password wrong.
        bail!(match code {
            "archive_password_required" if password.is_some() => "archive_password_wrong",
            code => code,
        });
    }
    Ok(output)
}

fn classify_process_error(exit: Option<i32>, stdout: &[u8], stderr: &[u8]) -> &'static str {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    )
    .to_ascii_lowercase();
    if text.contains("wrong password")
        || text.contains("password is incorrect")
        || text.contains("can not open encrypted archive")
    {
        "archive_password_required"
    } else if text.contains("missing volume")
        || text.contains("cannot find volume")
        || text.contains("can not find volume")
        || (text.contains("unexpected end of archive")
            && (text.contains("type = split") || text.contains("multivolume = +")))
    {
        "archive_missing_volume"
    } else if text.contains("not enough space")
        || text.contains("no space left")
        || text.contains("disk full")
    {
        "storage_low_space"
    } else {
        match exit {
            Some(7) => "archive_tool_arguments",
            Some(8) => "archive_tool_memory",
            Some(255) => "cancelled",
            _ => "archive_corrupt",
        }
    }
}

fn is_zip(archive: &Path) -> bool {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_lowercase();
    name.ends_with(".zip")
        || name.rsplit_once('.').is_some_and(|(base, suffix)| {
            base.ends_with(".zip")
                && suffix.len() >= 3
                && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// `code_page` decodes ZIP names that carry no UTF-8 marker; 7-Zip honors explicit
/// UTF-8/Unicode name metadata either way. Listing and extraction use the same one.
/// `-sccUTF-8` also reads a given `password` from the input as UTF-8.
fn cli_args(operation: &str, archive: &Path, code_page: u32, password: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        OsString::from(operation),
        "-y".into(),
        "-bd".into(),
        "-bb0".into(),
        "-sccUTF-8".into(),
    ]
    .into();
    if !password {
        // Never prompt for credentials. Encrypted members are rejected by preflight.
        args.push("-pNNA-no-password-supplied".into());
    }
    args.extend(["--".into(), archive.as_os_str().to_owned()]);
    if is_zip(archive) {
        args.insert(1, format!("-mcp={code_page}").into());
    }
    args
}

/// Unmarked ZIP names are read as UTF-8 when every name is valid UTF-8 (macOS and many
/// tools write them so); otherwise as Japanese Windows names (CP932), as BOOTH archives
/// made on Windows use. Returns the code page extraction must use with the entries.
fn list_archive(
    seven_zip: &Path,
    first: &Path,
    cancelled: &AtomicBool,
    password: Option<&str>,
) -> Result<(u32, Vec<ArchiveEntry>)> {
    let list = |code_page| -> Result<Vec<u8>> {
        let mut args = cli_args("l", first, code_page, password.is_some());
        args.insert(1, "-slt".into());
        run_7zip(seven_zip, &args, cancelled, true, None, password)
    };
    let parse = |listing: &[u8]| {
        parse_listing(
            std::str::from_utf8(listing).context("archive_listing_invalid")?,
            password.is_some(),
        )
    };
    if is_zip(first) {
        // A tool failure does not depend on name decoding; a name the listing rejects may.
        // 7-Zip shows each byte that is not valid UTF-8 as U+EF80..U+EFFF (or U+FFFD).
        let invalid = |c: char| c == '\u{FFFD}' || ('\u{EF80}'..='\u{EFFF}').contains(&c);
        if let Ok(entries) = parse(&list(65001)?) {
            if !entries.iter().any(|e| e.relative_path.contains(invalid)) {
                return Ok((65001, entries));
            }
        }
    }
    Ok((932, parse(&list(932)?)?))
}

/// Also adds each read's bytes to `checked`.
fn hash_file(
    path: &Path,
    cancelled: &AtomicBool,
    checked: &AtomicU64,
) -> Result<(u64, String, u32)> {
    let mut file = File::open(path).context("storage_read_failed")?;
    let mut digest = Sha256::new();
    let mut crc = !0u32;
    let mut size = 0u64;
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
        check_cancel(cancelled)?;
        let count = file.read(&mut buffer).context("storage_read_failed")?;
        if count == 0 {
            break;
        }
        checked.fetch_add(count as u64, Ordering::Relaxed);
        digest.update(&buffer[..count]);
        for byte in &buffer[..count] {
            crc = (crc >> 8) ^ CRC_TABLE[((crc ^ u32::from(*byte)) & 0xff) as usize];
        }
        size = size
            .checked_add(count as u64)
            .context("archive_size_overflow")?;
    }
    Ok((size, hex::encode(digest.finalize()), !crc))
}

fn copy_source(source: &Path, destination: &Path, cancelled: &AtomicBool) -> Result<()> {
    validate_no_links(source)?;
    ensure!(
        fs::metadata(source)
            .context("archive_missing_volume")?
            .is_file(),
        "archive_missing_volume"
    );
    let mut input = File::open(source).context("archive_missing_volume")?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .context("storage_write_failed")?;
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut hash = Sha256::new();
    loop {
        check_cancel(cancelled)?;
        let count = input.read(&mut buffer).context("storage_read_failed")?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        output
            .write_all(&buffer[..count])
            .context("storage_write_failed")?;
    }
    output.sync_all().context("storage_write_failed")?;
    drop(output);
    ensure!(
        hex::encode(hash.finalize()) == hash_file(destination, cancelled, &AtomicU64::new(0))?.1,
        "storage_copy_verification_failed"
    );
    Ok(())
}

fn reject_hard_links(file: &File, metadata: &fs::Metadata) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = file;
        ensure!(metadata.nlink() == 1, "archive_link_forbidden");
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        #[repr(C)]
        struct FileInfo {
            attributes: u32,
            creation: [u32; 2],
            access: [u32; 2],
            write: [u32; 2],
            volume: u32,
            size_high: u32,
            size_low: u32,
            links: u32,
            index_high: u32,
            index_low: u32,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetFileInformationByHandle(
                handle: *mut std::ffi::c_void,
                info: *mut FileInfo,
            ) -> i32;
        }
        let mut info = std::mem::MaybeUninit::<FileInfo>::uninit();
        // The OS initializes the repr(C) structure on success; file owns this live handle.
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } != 0,
            "storage_read_failed"
        );
        ensure!(
            unsafe { info.assume_init() }.links == 1,
            "archive_link_forbidden"
        );
        let _ = metadata;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (file, metadata);
        bail!("archive_platform_unsupported");
    }
    Ok(())
}

/// Verify exact files and directories, including implicit parent directories.
pub fn verify_extracted_inventory(
    root: &Path,
    expected: &[ArchiveEntry],
    cancelled: &AtomicBool,
) -> Result<Vec<VerifiedEntry>> {
    verify_inventory(root, expected, cancelled, &AtomicU64::new(0))
}

/// As `verify_extracted_inventory`, adding the bytes it reads to `checked`.
fn verify_inventory(
    root: &Path,
    expected: &[ArchiveEntry],
    cancelled: &AtomicBool,
    checked: &AtomicU64,
) -> Result<Vec<VerifiedEntry>> {
    validate_no_links(root)?;
    let mut remaining = validate_inventory_paths(expected)?;
    let expected_files: BTreeMap<_, _> = expected
        .iter()
        .filter(|v| !v.is_directory)
        .map(|v| (v.relative_path.as_str(), v))
        .collect();
    let mut pending = vec![root.to_owned()];
    let mut inventory = Vec::new();
    while let Some(directory) = pending.pop() {
        check_cancel(cancelled)?;
        for item in fs::read_dir(&directory).context("storage_read_failed")? {
            let item = item.context("storage_read_failed")?;
            let path = item.path();
            validate_no_links(&path)?;
            let relative = portable_path(path.strip_prefix(root).context("archive_unsafe_path")?)?;
            let is_directory = remaining
                .remove(&relative)
                .context("archive_inventory_mismatch")?;
            let metadata = fs::symlink_metadata(&path).context("storage_read_failed")?;
            ensure!(
                metadata.is_dir() == is_directory,
                "archive_inventory_mismatch"
            );
            if is_directory {
                inventory.push(VerifiedEntry {
                    relative_path: relative,
                    size_bytes: 0,
                    hash: String::new(),
                    is_directory: true,
                });
                pending.push(path);
                continue;
            }
            ensure!(metadata.is_file(), "archive_link_forbidden");
            // 7-Zip may restore a read-only flag. Temporarily allow flushing the
            // new staged file, then restore its original permissions.
            let original_permissions = metadata.permissions();
            #[cfg(windows)]
            if original_permissions.readonly() {
                let mut writable = original_permissions.clone();
                writable.set_readonly(false);
                fs::set_permissions(&path, writable).context("storage_write_failed")?;
            }
            let file = super::io::retry_busy(|| {
                OpenOptions::new()
                    .read(true)
                    .write(cfg!(windows))
                    .open(&path)
            })
            .context("storage_read_failed")?;
            reject_hard_links(&file, &metadata)?;
            let flushed = file.sync_all();
            drop(file);
            #[cfg(windows)]
            fs::set_permissions(&path, original_permissions).context("storage_write_failed")?;
            #[cfg(not(windows))]
            let _ = original_permissions;
            flushed.context("storage_write_failed")?;
            let (size, hash, crc) = hash_file(&path, cancelled, checked)?;
            let expected_file = expected_files
                .get(relative.as_str())
                .context("archive_inventory_mismatch")?;
            ensure!(
                size == expected_file.size_bytes,
                "archive_inventory_mismatch"
            );
            ensure!(
                expected_file
                    .crc32
                    .is_none_or(|expected_crc| expected_crc == crc),
                "archive_crc_mismatch"
            );
            inventory.push(VerifiedEntry {
                relative_path: relative,
                size_bytes: size,
                hash,
                is_directory: false,
            });
        }
    }
    ensure!(remaining.is_empty(), "archive_inventory_mismatch");
    inventory.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(inventory)
}

/// Produces a verified staging directory without expanding nested archives.
/// Failure/cancellation removes only this invocation's private staging tree.
/// An encrypted archive needs its `password`.
pub fn extract_archive(
    seven_zip: &Path,
    archive: &Path,
    available_volumes: &[PathBuf],
    staging_parent: &Path,
    cancelled: &AtomicBool,
    progress: &ExtractionProgress,
    password: Option<&str>,
) -> Result<ArchiveExtraction> {
    check_cancel(cancelled)?;
    validate_no_links(staging_parent)?;
    ensure!(staging_parent.is_dir(), "storage_staging_unavailable");
    let staging_parent = staging_parent
        .canonicalize()
        .context("storage_staging_unavailable")?;
    let sources = validate_volume_group(archive, available_volumes)?;
    let source_state = sources
        .iter()
        .map(|source| {
            validate_no_links(source)?;
            let metadata = fs::metadata(source).context("archive_missing_volume")?;
            ensure!(metadata.is_file(), "archive_missing_volume");
            Ok((
                metadata.len(),
                metadata.modified().context("storage_read_failed")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        fs2::available_space(&staging_parent).context("storage_space_unavailable")?
            >= 16 * 1024 * 1024,
        "storage_low_space"
    );
    let workspace_id = uuid::Uuid::new_v4();
    let workspace = staging_parent.join(format!("archive-{workspace_id}"));
    fs::create_dir(&workspace).context("storage_write_failed")?;
    let mut guard = WorkspaceGuard {
        root: workspace.clone(),
        parent: staging_parent,
        armed: true,
        owner: None,
    };
    let mut owner = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(workspace.join(WORKSPACE_MARKER))
        .context("storage_write_failed")?;
    fs2::FileExt::try_lock_exclusive(&owner).context("storage_write_failed")?;
    owner
        .write_all(format!("{WORKSPACE_OWNER}{workspace_id}\n").as_bytes())
        .context("storage_write_failed")?;
    owner.sync_all().context("storage_write_failed")?;
    guard.owner = Some(owner);
    let inputs = workspace.join("sources");
    let content = workspace.join("content");
    fs::create_dir(&content).context("storage_write_failed")?;
    let first = first_volume(&sources, &inputs, &workspace, cancelled)?;
    let (code_page, entries) = list_archive(seven_zip, &first, cancelled, password)?;
    let plan = plan_extraction(&entries);
    let expanded_size = entries.iter().try_fold(0u64, |total, entry| {
        total
            .checked_add(entry.size_bytes)
            .context("archive_size_overflow")
    })?;
    ensure!(
        fs2::available_space(&workspace).context("storage_space_unavailable")?
            >= expanded_size.saturating_add(16 * 1024 * 1024),
        "storage_low_space"
    );
    progress.size.store(expanded_size, Ordering::Relaxed);
    // Bind writes to literal validated file names. Technical listings are text:
    // a malicious name containing a newline must never smuggle an unlisted
    // member into extraction. Never select a directory recursively.
    let member_list = workspace.join("members.txt");
    let mut names = BufWriter::new(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&member_list)
            .context("storage_write_failed")?,
    );
    // Every folder exists, in its first-seen spelling, before 7-Zip writes a file.
    for entry in &plan.outputs {
        check_cancel(cancelled)?;
        let path = content.join(validate_relative_path(&entry.relative_path)?);
        let folder = if entry.is_directory {
            Some(path.as_path())
        } else {
            path.parent()
        };
        fs::create_dir_all(folder.context("archive_unsafe_path")?)
            .context("storage_write_failed")?;
    }
    let mut file_count = 0usize;
    for entry in entries.iter().filter(|entry| !entry.is_directory) {
        // 7-Zip trims unquoted listfile lines. Quotes retain a leading
        // space in a literal member name; validated paths cannot contain quotes.
        writeln!(names, "\"{}\"", entry.relative_path).context("storage_write_failed")?;
        file_count += 1;
    }
    names.flush().context("storage_write_failed")?;
    drop(names);
    if file_count > 0 {
        let mut extraction_args = cli_args("x", &first, code_page, password.is_some());
        for arg in extraction_args
            .iter_mut()
            .filter(|arg| arg.as_os_str() == "-bd")
        {
            *arg = "-bsp1".into();
        }
        let mut output_arg = OsString::from("-o");
        output_arg.push(&content);
        let mut include_arg = OsString::from("-i@");
        include_arg.push(&member_list);
        extraction_args.splice(
            1..1,
            [
                output_arg,
                include_arg,
                "-spd".into(),
                "-r-".into(),
                "-scsUTF-8".into(),
                // A taken name gets `_1`, `_2`…; nothing is overwritten.
                "-aou".into(),
            ],
        );
        run_7zip(
            seven_zip,
            &extraction_args,
            cancelled,
            false,
            Some(progress),
            password,
        )?;
    }
    progress.written.store(100, Ordering::Relaxed);
    let mut inventory = verify_inventory(&content, &plan.outputs, cancelled, &progress.checked)?;
    // Keep one of identical copies; a CRC match must also be a content match.
    for (copy, kept) in &plan.identical {
        let hash = |path: &String| {
            inventory
                .iter()
                .find(|entry| entry.relative_path == *path)
                .map(|entry| entry.hash.clone())
        };
        if hash(copy).is_some() && hash(copy) == hash(kept) {
            let path = content.join(validate_relative_path(copy)?);
            // 7-Zip may have restored a read-only flag on this staged copy.
            #[cfg(windows)]
            {
                let mut permissions = fs::metadata(&path)
                    .context("storage_read_failed")?
                    .permissions();
                if permissions.readonly() {
                    permissions.set_readonly(false);
                    fs::set_permissions(&path, permissions).context("storage_write_failed")?;
                }
            }
            super::io::retry_busy(|| fs::remove_file(&path)).context("storage_write_failed")?;
            inventory.retain(|entry| entry.relative_path != *copy);
        }
    }
    check_cancel(cancelled)?;
    for (source, state) in sources.iter().zip(source_state) {
        validate_no_links(source)?;
        let metadata = fs::metadata(source).context("archive_missing_volume")?;
        ensure!(
            metadata.is_file()
                && metadata.len() == state.0
                && metadata.modified().context("storage_read_failed")? == state.1,
            "file_integrity_failed"
        );
    }
    if inputs.exists() {
        remove_private_tree(&inputs)?;
    }
    super::io::retry_busy(|| fs::remove_file(&member_list))
        .context("storage_staging_cleanup_failed")?;
    Ok(ArchiveExtraction {
        staging_path: content,
        inventory,
        source_paths: sources,
        _workspace: guard,
    })
}

/// The first volume 7-Zip reads: in place for one complete archive, else a private link or copy.
fn first_volume(
    sources: &[PathBuf],
    inputs: &Path,
    workspace: &Path,
    cancelled: &AtomicBool,
) -> Result<PathBuf> {
    let first_name = sources[0]
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(volume_name)
        .context("archive_invalid_name")?;
    // Keep split input sets explicit: 7-Zip otherwise discovers adjacent final
    // volumes that the caller did not include, hiding a missing-volume claim.
    let direct = sources.len() == 1
        && first_name.index == 0
        && !fs::read_dir(sources[0].parent().context("archive_invalid_name")?)
            .context("archive_missing_volume")?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().to_str().and_then(volume_name))
            .any(|name| {
                name.key == first_name.key
                    && name.kind == first_name.kind
                    && name.index != first_name.index
            });
    let first = if direct {
        // Ordinary complete single archives are read in place.
        sources[0].clone()
    } else {
        fs::create_dir(inputs).context("storage_write_failed")?;
        for source in sources {
            check_cancel(cancelled)?;
            let name = source.file_name().context("archive_invalid_name")?;
            let destination = inputs.join(name);
            // Group aliases need no payload copy on filesystems supporting
            // hard links. Read-only inputs use the existing verified copy path,
            // so helper cleanup never changes their attributes via an alias.
            if fs::metadata(source)
                .context("archive_missing_volume")?
                .permissions()
                .readonly()
                || fs::hard_link(source, &destination).is_err()
            {
                let size = fs::metadata(source)
                    .context("archive_missing_volume")?
                    .len();
                ensure!(
                    fs2::available_space(workspace).context("storage_space_unavailable")?
                        >= size.saturating_add(16 * 1024 * 1024),
                    "storage_low_space"
                );
                copy_source(source, &destination, cancelled)?;
            }
        }
        inputs.join(sources[0].file_name().context("archive_invalid_name")?)
    };
    Ok(first)
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
