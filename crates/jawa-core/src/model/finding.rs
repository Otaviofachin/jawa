use std::fmt;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::model::stored_path::to_hex;
use crate::system::processes::{Holder, describe_holders};

const ID_LENGTH: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CategoryId {
    #[serde(rename = "npm-cache")]
    NpmCache,
    #[serde(rename = "caches-ferramentas")]
    ToolCaches,
    #[serde(rename = "caches-apps")]
    AppCaches,
    #[serde(rename = "saidas-build")]
    BuildOutputs,
    #[serde(rename = "rust-target")]
    RustTargets,
    #[serde(rename = "node-modules-parados")]
    IdleNodeModules,
    #[serde(rename = "nvm-versoes")]
    NvmVersions,
    #[serde(rename = "docker-imagens")]
    DockerImages,
    #[serde(rename = "docker-volumes")]
    DockerVolumes,
    #[serde(rename = "docker-desktop-disco")]
    DockerDesktopDisk,
    #[serde(rename = "downloads")]
    Downloads,
    #[serde(rename = "sistema")]
    System,
}

impl CategoryId {
    pub const ALL: [CategoryId; 12] = [
        CategoryId::NpmCache,
        CategoryId::ToolCaches,
        CategoryId::AppCaches,
        CategoryId::BuildOutputs,
        CategoryId::RustTargets,
        CategoryId::IdleNodeModules,
        CategoryId::NvmVersions,
        CategoryId::DockerImages,
        CategoryId::DockerVolumes,
        CategoryId::DockerDesktopDisk,
        CategoryId::Downloads,
        CategoryId::System,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CategoryId::NpmCache => "npm-cache",
            CategoryId::ToolCaches => "caches-ferramentas",
            CategoryId::AppCaches => "caches-apps",
            CategoryId::BuildOutputs => "saidas-build",
            CategoryId::RustTargets => "rust-target",
            CategoryId::IdleNodeModules => "node-modules-parados",
            CategoryId::NvmVersions => "nvm-versoes",
            CategoryId::DockerImages => "docker-imagens",
            CategoryId::DockerVolumes => "docker-volumes",
            CategoryId::DockerDesktopDisk => "docker-desktop-disco",
            CategoryId::Downloads => "downloads",
            CategoryId::System => "sistema",
        }
    }
}

impl fmt::Display for CategoryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("categoria desconhecida: {0}")]
pub struct UnknownCategory(pub String);

impl FromStr for CategoryId {
    type Err = UnknownCategory;

    fn from_str(text: &str) -> Result<CategoryId, UnknownCategory> {
        CategoryId::ALL
            .into_iter()
            .find(|id| id.as_str() == text)
            .ok_or_else(|| UnknownCategory(text.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Risk {
    Safe,
    Review,
    Guided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reversibility {
    Regenerable,
    Trash,
    Irreversible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    #[serde(with = "crate::model::stored_path")]
    pub program: PathBuf,
    pub args: Vec<String>,
    pub stdin: Option<Vec<u8>>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

pub const DOCKER_DESKTOP_CONTEXT: &str = "desktop-linux";
pub const DOCKER_CONTEXT_VAR: &str = "DOCKER_CONTEXT";

impl CommandSpec {
    pub fn runs_in_docker_desktop(&self) -> bool {
        self.env
            .iter()
            .any(|(name, value)| name == DOCKER_CONTEXT_VAR && value == DOCKER_DESKTOP_CONTEXT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuideSpec {
    pub command_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Delete(#[serde(with = "crate::model::stored_path")] PathBuf),
    Trash(#[serde(with = "crate::model::stored_path")] PathBuf),
    Run(CommandSpec),
    Guide(GuideSpec),
}

impl Action {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Action::Delete(path) | Action::Trash(path) => Some(path),
            Action::Run(_) | Action::Guide(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub device: u64,
    pub inode: u64,
    pub modified_secs: i64,
}

pub fn fingerprint_of(path: &Path) -> io::Result<Fingerprint> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(Fingerprint {
        device: metadata.dev(),
        inode: metadata.ino(),
        modified_secs: metadata.mtime(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockReason {
    InUse(Vec<Holder>),
    ForeignOwned { count: u64, fix: String },
    CrossesMount,
}

impl fmt::Display for BlockReason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockReason::InUse(holders) => {
                write!(formatter, "em uso por {}", describe_holders(holders))
            }
            BlockReason::ForeignOwned { count, fix } => write!(
                formatter,
                "{count} arquivo(s) de outro dono; para corrigir: {fix}"
            ),
            BlockReason::CrossesMount => formatter.write_str("atravessa um ponto de montagem"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("id de item inválido: {0} (são {ID_LENGTH} caracteres hexadecimais)")]
pub struct InvalidFindingId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FindingId(String);

impl FindingId {
    pub fn of(category: CategoryId, action: &Action) -> FindingId {
        let mut hasher = Sha256::new();
        hasher.update(category.as_str().as_bytes());
        hasher.update([0u8]);
        hasher.update(action_key(action));
        let mut hex = to_hex(&hasher.finalize());
        hex.truncate(ID_LENGTH);
        FindingId(hex)
    }
}

fn action_key(action: &Action) -> Vec<u8> {
    match action {
        Action::Delete(path) | Action::Trash(path) => path.as_os_str().as_bytes().to_vec(),
        Action::Run(spec) => {
            let mut key = spec.program.as_os_str().as_bytes().to_vec();
            for arg in &spec.args {
                key.push(0);
                key.extend_from_slice(arg.as_bytes());
            }
            key
        }
        Action::Guide(guide) => guide.command_line.as_bytes().to_vec(),
    }
}

impl FromStr for FindingId {
    type Err = InvalidFindingId;

    fn from_str(text: &str) -> Result<FindingId, InvalidFindingId> {
        let is_valid =
            text.len() == ID_LENGTH && text.bytes().all(|digit| digit.is_ascii_hexdigit());
        if !is_valid {
            return Err(InvalidFindingId(text.to_string()));
        }
        Ok(FindingId(text.to_ascii_lowercase()))
    }
}

impl TryFrom<String> for FindingId {
    type Error = InvalidFindingId;

    fn try_from(text: String) -> Result<FindingId, InvalidFindingId> {
        text.parse()
    }
}

impl From<FindingId> for String {
    fn from(id: FindingId) -> String {
        id.0
    }
}

impl fmt::Display for FindingId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub id: FindingId,
    pub category: CategoryId,
    pub label: String,
    pub location: String,
    pub bytes: u64,
    pub risk: Risk,
    pub reversibility: Reversibility,
    pub note: Option<String>,
    pub action: Action,
    pub fingerprint: Option<Fingerprint>,
    pub blocked: Option<BlockReason>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const NPM: CategoryId = CategoryId::NpmCache;
    const TOOLS: CategoryId = CategoryId::ToolCaches;

    fn delete(path: &str) -> Action {
        Action::Delete(PathBuf::from(path))
    }

    #[test]
    fn id_is_stable_and_short() {
        let first = FindingId::of(NPM, &delete("/home/eu/.npm/_cacache"));
        let second = FindingId::of(NPM, &delete("/home/eu/.npm/_cacache"));
        assert_eq!(first, second);
        assert_eq!(first.to_string().len(), 10);
        assert!(first.to_string().chars().all(|c| c.is_ascii_hexdigit()));
        let other = FindingId::of(NPM, &delete("/home/eu/.npm/_npx"));
        assert_ne!(first, other);
    }

    #[test]
    fn id_differs_by_category() {
        let action = delete("/home/eu/.cache/yarn");
        assert_ne!(FindingId::of(NPM, &action), FindingId::of(TOOLS, &action));
    }

    #[test]
    fn id_covers_command_args_and_guide_line() {
        let run = |args: &[&str]| {
            Action::Run(CommandSpec {
                program: PathBuf::from("/usr/bin/docker"),
                args: args.iter().map(|arg| arg.to_string()).collect(),
                stdin: None,
                env: Vec::new(),
                timeout: Duration::from_secs(1),
            })
        };
        let volume_a = FindingId::of(NPM, &run(&["volume", "rm", "a"]));
        let volume_b = FindingId::of(NPM, &run(&["volume", "rm", "b"]));
        assert_ne!(volume_a, volume_b);
        let guide = |line: &str| {
            Action::Guide(GuideSpec {
                command_line: line.to_string(),
            })
        };
        assert_ne!(
            FindingId::of(NPM, &guide("sudo apt-get clean")),
            FindingId::of(NPM, &guide("sudo journalctl --vacuum-size=200M"))
        );
    }

    #[test]
    fn id_parses_and_displays() {
        let id = FindingId::of(NPM, &delete("/x"));
        let parsed: FindingId = id.to_string().parse().unwrap();
        assert_eq!(parsed, id);
        assert!("abc".parse::<FindingId>().is_err());
        assert!("zzzzzzzzzz".parse::<FindingId>().is_err());
        assert_eq!(
            "ABCDEF0123".parse::<FindingId>().unwrap().to_string(),
            "abcdef0123"
        );
    }

    #[test]
    fn fingerprint_changes_after_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("alvo");
        fs::create_dir(&target).unwrap();
        let before = fingerprint_of(&target).unwrap();
        assert_eq!(fingerprint_of(&target).unwrap(), before);
        fs::rename(&target, dir.path().join("antigo")).unwrap();
        fs::create_dir(&target).unwrap();
        let after = fingerprint_of(&target).unwrap();
        assert!(after.inode != before.inode || after.modified_secs != before.modified_secs);
    }

    #[test]
    fn fingerprint_does_not_follow_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_ne!(
            fingerprint_of(&link).unwrap(),
            fingerprint_of(&real).unwrap()
        );
    }

    #[test]
    fn fingerprint_of_missing_is_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(fingerprint_of(&dir.path().join("nada")).is_err());
    }

    #[test]
    fn block_reasons_read_in_portuguese() {
        let holders = vec![Holder {
            pid: 12,
            name: "node".to_string(),
        }];
        assert_eq!(
            BlockReason::InUse(holders).to_string(),
            "em uso por node (PID 12)"
        );
        assert!(
            BlockReason::ForeignOwned {
                count: 3,
                fix: "sudo chown -R \"$USER\": 'x'".to_string()
            }
            .to_string()
            .contains("3 arquivo(s) de outro dono")
        );
        assert!(BlockReason::CrossesMount.to_string().contains("montagem"));
    }
}
