use crate::model::finding::{Action, CategoryId, Finding, Reversibility};

const REINSTALL_DOWNLOADS: &str = "As reinstalações vão baixar tudo da internet.";
const PRIVATE_REGISTRY: &str =
    "Algum projeto usa registry privado: confira se o token ainda vale antes de reinstalar.";
const TRASH_SPACE: &str = "O espaço só volta quando você esvaziar a Lixeira.";
const PRIVATE_REGISTRY_MARK: &str = "registry privado";

pub fn selection_warnings(selected: &[&Finding]) -> Vec<&'static str> {
    let has_category = |id: CategoryId| selected.iter().any(|finding| finding.category == id);
    let mut warnings = Vec::new();
    if has_category(CategoryId::NpmCache) && has_category(CategoryId::IdleNodeModules) {
        warnings.push(REINSTALL_DOWNLOADS);
    }
    if selected
        .iter()
        .any(|finding| mentions_private_registry(finding))
    {
        warnings.push(PRIVATE_REGISTRY);
    }
    if selected.iter().any(|finding| goes_to_trash(finding)) {
        warnings.push(TRASH_SPACE);
    }
    warnings
}

fn mentions_private_registry(finding: &Finding) -> bool {
    finding
        .note
        .as_deref()
        .is_some_and(|note| note.contains(PRIVATE_REGISTRY_MARK))
}

fn goes_to_trash(finding: &Finding) -> bool {
    finding.reversibility == Reversibility::Trash || matches!(finding.action, Action::Trash(_))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::finding::{
        Action, BlockReason, CategoryId, Finding, FindingId, Reversibility, Risk,
    };
    use std::path::PathBuf;

    fn finding(category: CategoryId, action: Action, note: Option<&str>) -> Finding {
        Finding {
            id: FindingId::of(category, &action),
            category,
            label: "item".to_string(),
            location: "~/x".to_string(),
            bytes: 2_000_000,
            risk: Risk::Safe,
            reversibility: Reversibility::Regenerable,
            note: note.map(str::to_string),
            action,
            fingerprint: None,
            blocked: None::<BlockReason>,
        }
    }

    fn delete(category: CategoryId, path: &str, note: Option<&str>) -> Finding {
        finding(category, Action::Delete(PathBuf::from(path)), note)
    }

    #[test]
    fn warnings_for_npm_plus_node_modules() {
        let npm = delete(CategoryId::NpmCache, "/h/.npm/_cacache", None);
        let modules = delete(CategoryId::IdleNodeModules, "/h/p/node_modules", None);
        assert_eq!(
            selection_warnings(&[&npm, &modules]),
            vec!["As reinstalações vão baixar tudo da internet."]
        );
        assert!(selection_warnings(&[&npm]).is_empty());
        assert!(selection_warnings(&[&modules]).is_empty());
    }

    #[test]
    fn warnings_for_private_registry() {
        let modules = delete(
            CategoryId::IdleNodeModules,
            "/h/p/node_modules",
            Some("npm ci. Atenção: o projeto usa registry privado"),
        );
        assert_eq!(
            selection_warnings(&[&modules]),
            vec![
                "Algum projeto usa registry privado: confira se o token ainda vale antes de reinstalar."
            ]
        );
    }

    #[test]
    fn warnings_for_trash() {
        let mut download = finding(
            CategoryId::Downloads,
            Action::Trash(PathBuf::from("/h/Downloads/a.iso")),
            None,
        );
        assert_eq!(
            selection_warnings(&[&download]),
            vec!["O espaço só volta quando você esvaziar a Lixeira."]
        );
        download.action = Action::Delete(PathBuf::from("/h/Downloads/a.iso"));
        download.reversibility = Reversibility::Trash;
        assert_eq!(selection_warnings(&[&download]).len(), 1);
    }

    #[test]
    fn warnings_keep_spec_order_and_dedupe() {
        let npm = delete(CategoryId::NpmCache, "/h/.npm/_cacache", None);
        let modules = delete(
            CategoryId::IdleNodeModules,
            "/h/p/node_modules",
            Some("registry privado"),
        );
        let other = delete(
            CategoryId::IdleNodeModules,
            "/h/q/node_modules",
            Some("registry privado"),
        );
        let trash = finding(
            CategoryId::Downloads,
            Action::Trash(PathBuf::from("/h/Downloads/a.iso")),
            None,
        );
        assert_eq!(
            selection_warnings(&[&npm, &modules, &other, &trash]).len(),
            3
        );
    }
}
