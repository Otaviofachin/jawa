use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
    pub pid: u32,
    pub name: String,
}

impl std::fmt::Display for Holder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} (PID {})", self.name, self.pid)
    }
}

pub fn holders_of(target: &Path, proc_root: &Path) -> Vec<Holder> {
    let target = resolve_parent(target);
    let own_pid = std::process::id();
    process_dirs(proc_root)
        .into_iter()
        .filter(|(pid, _)| *pid != own_pid)
        .filter(|(_, process_dir)| touches_target(process_dir, &target))
        .map(|(pid, process_dir)| Holder {
            pid,
            name: process_name(&process_dir).unwrap_or_default(),
        })
        .collect()
}

pub fn running_named(names: &[&str], proc_root: &Path) -> Vec<Holder> {
    if names.is_empty() {
        return Vec::new();
    }
    process_dirs(proc_root)
        .into_iter()
        .filter_map(|(pid, process_dir)| {
            let name = process_name(&process_dir)?;
            names
                .contains(&name.as_str())
                .then_some(Holder { pid, name })
        })
        .collect()
}

fn process_dirs(proc_root: &Path) -> Vec<(u32, PathBuf)> {
    let Ok(entries) = fs::read_dir(proc_root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
            Some((pid, entry.path()))
        })
        .collect()
}

fn touches_target(process_dir: &Path, target: &Path) -> bool {
    ["cwd", "exe"].iter().any(|link| {
        fs::read_link(process_dir.join(link)).is_ok_and(|linked| linked.starts_with(target))
    })
}

fn process_name(process_dir: &Path) -> Option<String> {
    let comm = fs::read_to_string(process_dir.join("comm")).ok()?;
    Some(comm.trim_end_matches('\n').to_string())
}

fn resolve_parent(target: &Path) -> PathBuf {
    match (target.parent(), target.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map_or_else(|_| target.to_path_buf(), |parent| parent.join(name)),
        _ => target.to_path_buf(),
    }
}

pub fn describe_holders(holders: &[Holder]) -> String {
    let mut groups: Vec<(&str, Vec<u32>)> = Vec::new();
    for holder in holders {
        match groups.iter_mut().find(|(name, _)| *name == holder.name) {
            Some((_, pids)) => pids.push(holder.pid),
            None => groups.push((&holder.name, vec![holder.pid])),
        }
    }
    groups
        .iter()
        .map(|(name, pids)| match pids.as_slice() {
            [pid] => format!("{name} (PID {pid})"),
            _ => format!("{name} ({} processos)", pids.len()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    struct FakeProc {
        _dir: tempfile::TempDir,
        home: PathBuf,
        proc_root: PathBuf,
    }

    fn fake_proc() -> FakeProc {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap();
        let proc_root = home.join("proc");
        fs::create_dir_all(home.join("projeto/sub")).unwrap();
        fs::create_dir_all(home.join("outro")).unwrap();
        fs::create_dir_all(&proc_root).unwrap();
        FakeProc {
            _dir: dir,
            home,
            proc_root,
        }
    }

    fn add_process(proc_root: &Path, pid: &str, link: &str, target: &Path, comm: &str) {
        let process_dir = proc_root.join(pid);
        fs::create_dir_all(&process_dir).unwrap();
        symlink(target, process_dir.join(link)).unwrap();
        fs::write(process_dir.join("comm"), comm).unwrap();
    }

    #[test]
    fn finds_process_with_cwd_inside() {
        let fx = fake_proc();
        add_process(
            &fx.proc_root,
            "123",
            "cwd",
            &fx.home.join("projeto/sub"),
            "node\n",
        );
        let holders = holders_of(&fx.home.join("projeto"), &fx.proc_root);
        assert_eq!(
            holders,
            vec![Holder {
                pid: 123,
                name: "node".to_string()
            }]
        );
    }

    #[test]
    fn finds_process_with_exe_inside() {
        let fx = fake_proc();
        add_process(
            &fx.proc_root,
            "77",
            "exe",
            &fx.home.join("projeto/sub/bin"),
            "server\n",
        );
        let holders = holders_of(&fx.home.join("projeto"), &fx.proc_root);
        assert_eq!(holders.len(), 1);
        assert_eq!(holders[0].pid, 77);
        assert_eq!(holders[0].name, "server");
    }

    #[test]
    fn ignores_process_outside() {
        let fx = fake_proc();
        add_process(
            &fx.proc_root,
            "123",
            "cwd",
            &fx.home.join("outro"),
            "node\n",
        );
        add_process(
            &fx.proc_root,
            "124",
            "cwd",
            &fx.home.join("projeto-irmao"),
            "node\n",
        );
        let holders = holders_of(&fx.home.join("projeto"), &fx.proc_root);
        assert!(holders.is_empty());
    }

    #[test]
    fn ignores_non_numeric_entries() {
        let fx = fake_proc();
        add_process(
            &fx.proc_root,
            "self",
            "cwd",
            &fx.home.join("projeto"),
            "x\n",
        );
        fs::write(fx.proc_root.join("uptime"), "1 2").unwrap();
        let holders = holders_of(&fx.home.join("projeto"), &fx.proc_root);
        assert!(holders.is_empty());
        assert!(running_named(&["x"], &fx.proc_root).is_empty());
    }

    #[test]
    fn ignores_own_pid() {
        let fx = fake_proc();
        let own_pid = std::process::id().to_string();
        add_process(
            &fx.proc_root,
            &own_pid,
            "cwd",
            &fx.home.join("projeto"),
            "eu\n",
        );
        assert!(holders_of(&fx.home.join("projeto"), &fx.proc_root).is_empty());
    }

    #[test]
    fn matches_running_by_name() {
        let fx = fake_proc();
        add_process(
            &fx.proc_root,
            "123",
            "cwd",
            &fx.home.join("outro"),
            "node\n",
        );
        add_process(
            &fx.proc_root,
            "200",
            "cwd",
            &fx.home.join("outro"),
            "slack\n",
        );
        let running = running_named(&["slack", "code"], &fx.proc_root);
        assert_eq!(
            running,
            vec![Holder {
                pid: 200,
                name: "slack".to_string()
            }]
        );
    }

    #[test]
    fn real_proc_lists_self_name() {
        let own_name = fs::read_to_string("/proc/self/comm").unwrap();
        let own_name = own_name.trim_end_matches('\n');
        let running = running_named(&[own_name], Path::new("/proc"));
        assert!(
            running
                .iter()
                .any(|holder| holder.pid == std::process::id())
        );
    }

    #[test]
    fn missing_proc_root_yields_nothing() {
        let fx = fake_proc();
        let missing = fx.home.join("nao-existe");
        assert!(holders_of(&fx.home.join("projeto"), &missing).is_empty());
        assert!(running_named(&["node"], &missing).is_empty());
    }

    #[test]
    fn describes_holders_grouped_by_name() {
        let holder = |pid: u32, name: &str| Holder {
            pid,
            name: name.to_string(),
        };
        let many_chrome: Vec<Holder> = (1..=12).map(|pid| holder(pid, "chrome")).collect();
        assert_eq!(describe_holders(&many_chrome), "chrome (12 processos)");
        let mixed = vec![holder(7, "node"), holder(8, "code"), holder(9, "code")];
        assert_eq!(describe_holders(&mixed), "node (PID 7), code (2 processos)");
    }
}
