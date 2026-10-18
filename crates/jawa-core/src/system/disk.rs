use std::io;
use std::path::Path;

/// Espaco livre para usuarios comuns (`f_bavail * f_frsize`).
pub fn free_bytes(path: &Path) -> io::Result<u64> {
    let stats = rustix::fs::statvfs(path).map_err(io::Error::from)?;
    Ok(stats.f_bavail.saturating_mul(stats.f_frsize))
}

/// Tamanho total do sistema de arquivos (`f_blocks * f_frsize`).
pub fn total_bytes(path: &Path) -> io::Result<u64> {
    let stats = rustix::fs::statvfs(path).map_err(io::Error::from)?;
    Ok(stats.f_blocks.saturating_mul(stats.f_frsize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_bytes_is_positive_and_not_above_total() {
        let dir = tempfile::tempdir().unwrap();
        let free = free_bytes(dir.path()).unwrap();
        let stats = rustix::fs::statvfs(dir.path()).unwrap();
        assert!(free > 0);
        assert!(free <= stats.f_blocks * stats.f_frsize);
    }

    #[test]
    fn total_bytes_is_at_least_free() {
        let dir = tempfile::tempdir().unwrap();
        let total = total_bytes(dir.path()).unwrap();
        assert!(total > 0);
        assert!(total >= free_bytes(dir.path()).unwrap());
    }
}
