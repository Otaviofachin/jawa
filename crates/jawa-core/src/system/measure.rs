use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use walkdir::WalkDir;

const BLOCK_SIZE: u64 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Measurement {
    pub allocated_bytes: u64,
    pub entries: u64,
    pub foreign_owned: u64,
    pub crosses_mount: bool,
}

/// Mede o espaco realmente alocado (`st_blocks * 512`) sem seguir symlinks
/// nem atravessar pontos de montagem.
pub fn measure(path: &Path, owner_uid: u32) -> io::Result<Measurement> {
    let root_metadata = fs::symlink_metadata(path)?;
    if root_metadata.is_dir() {
        fs::read_dir(path)?;
    }
    let mut measurement = Measurement::default();
    let mut seen_links: HashSet<(u64, u64)> = HashSet::new();
    let mut walker = WalkDir::new(path)
        .follow_links(false)
        .follow_root_links(false)
        .into_iter();
    while let Some(next) = walker.next() {
        let Ok(entry) = next else {
            measurement.foreign_owned += 1;
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            measurement.foreign_owned += 1;
            continue;
        };
        if metadata.is_dir() && metadata.dev() != root_metadata.dev() {
            measurement.crosses_mount = true;
            walker.skip_current_dir();
            continue;
        }
        record_entry(&mut measurement, &mut seen_links, &metadata, owner_uid);
    }
    Ok(measurement)
}

fn record_entry(
    measurement: &mut Measurement,
    seen_links: &mut HashSet<(u64, u64)>,
    metadata: &Metadata,
    owner_uid: u32,
) {
    measurement.entries += 1;
    if metadata.uid() != owner_uid {
        measurement.foreign_owned += 1;
    }
    let is_repeated_link = metadata.nlink() > 1
        && !metadata.is_dir()
        && !seen_links.insert((metadata.dev(), metadata.ino()));
    if !is_repeated_link {
        measurement.allocated_bytes += metadata.blocks() * BLOCK_SIZE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    const KIB: usize = 1024;

    fn current_uid() -> u32 {
        rustix::process::geteuid().as_raw()
    }

    fn write_file(path: &Path, size: usize) {
        fs::write(path, vec![7u8; size]).unwrap();
    }

    #[test]
    fn sums_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        write_file(&dir.path().join("a.bin"), 256 * KIB);
        write_file(&dir.path().join("sub/b.bin"), 256 * KIB);
        let result = measure(dir.path(), current_uid()).unwrap();
        assert!(result.allocated_bytes >= 512 * 1024);
        assert_eq!(result.foreign_owned, 0);
        assert!(!result.crosses_mount);
    }

    #[test]
    fn counts_hard_link_once() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.bin"), 256 * KIB);
        let single = measure(dir.path(), current_uid()).unwrap();
        fs::hard_link(dir.path().join("a.bin"), dir.path().join("b.bin")).unwrap();
        let linked = measure(dir.path(), current_uid()).unwrap();
        assert_eq!(linked.allocated_bytes, single.allocated_bytes);
        assert_eq!(linked.entries, single.entries + 1);
    }

    #[test]
    fn ignores_symlink_targets() {
        let outside = tempfile::tempdir().unwrap();
        write_file(&outside.path().join("big.bin"), 1024 * KIB);
        let dir = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.path().join("link")).unwrap();
        let result = measure(dir.path(), current_uid()).unwrap();
        assert!(result.allocated_bytes < 512 * 1024);
    }

    #[test]
    fn counts_foreign_owned_entries() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir.path().join("a.bin"), KIB);
        let result = measure(dir.path(), current_uid() + 1).unwrap();
        assert!(result.entries >= 2);
        assert_eq!(result.foreign_owned, result.entries);
    }

    #[test]
    fn unreadable_root_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = measure(&locked, current_uid());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
    }

    #[test]
    fn unreadable_subdirectory_counts_as_foreign() {
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let result = measure(dir.path(), current_uid());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.unwrap().foreign_owned >= 1);
    }

    #[test]
    fn does_not_follow_root_symlink() {
        let outside = tempfile::tempdir().unwrap();
        write_file(&outside.path().join("big.bin"), 1024 * KIB);
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link");
        symlink(outside.path(), &link).unwrap();
        let result = measure(&link, current_uid()).unwrap();
        assert_eq!(result.entries, 1);
        assert!(result.allocated_bytes < 512 * 1024);
    }
}
