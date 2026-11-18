use super::{MatchedDeclaration, SelectorBudget, StyleRule};
use crate::SvgError;
use crate::xml::XmlElement;

/// Returns every declaration that applies to `element`, in cascade order
/// (lowest specificity/order first; callers apply later entries last).
pub(crate) fn matching_declarations<'a>(
    rules: &'a [StyleRule],
    element: &XmlElement,
    ancestors: &[&XmlElement],
    budget: &mut SelectorBudget,
) -> Result<Vec<MatchedDeclaration<'a>>, SvgError> {
    let mut matched = Vec::new();
    for rule in rules {
        let mut best_specificity: Option<u32> = None;
        for selector in &rule.selectors {
            budget.charge()?;
            if selector.matches(element, ancestors) {
                let specificity = selector.specificity();
                best_specificity =
                    Some(best_specificity.map_or(specificity, |best| best.max(specificity)));
            }
        }
        let Some(specificity) = best_specificity else {
            continue;
        };
        for declaration in &rule.declarations {
            matched.push(MatchedDeclaration {
                specificity,
                order: rule.order,
                name: declaration.name.as_str(),
                value: declaration.value.as_str(),
            });
        }
    }
    matched.sort_by_key(|declaration| (declaration.specificity, declaration.order));
    Ok(matched)
}
