//! Cross-fork convergence proof: Redmine and OpenProject, two ends of the
//! same lineage (Redmine → ChiliProject → OpenProject), mint the **same
//! codebook id** for every shared canonical concept — even when the Rails
//! class names diverged across the fork.
//!
//! The artifact ([`FORK_CONVERGENCE_JSON`]) is produced by running the OGAR
//! producer over *both* real corpora and grouping promoted concepts by id.
//! It is the executable form of the "transcode into the same project
//! domain as a smoke test" idea.

use serde::Deserialize;

/// The vendored cross-fork convergence artifact.
pub const FORK_CONVERGENCE_JSON: &str = include_str!("../data/fork_convergence.json");

/// One canonical concept seen from the two-fork vantage point.
#[derive(Debug, Clone, Deserialize)]
pub struct ConvergedConcept {
    /// Canonical concept name (e.g. `project_work_item`).
    pub concept: String,
    /// Codebook id as a `0xDDCC` hex string.
    pub class_id: String,
    /// Codebook id as 2 little-endian bytes.
    pub class_id_le: [u8; 2],
    /// Redmine class name(s) that resolve to this concept.
    pub redmine: Vec<String>,
    /// OpenProject class name(s) that resolve to this concept.
    pub openproject: Vec<String>,
}

impl ConvergedConcept {
    /// The codebook id as a `u16`.
    #[must_use]
    pub fn class_id_u16(&self) -> u16 {
        u16::from_le_bytes(self.class_id_le)
    }

    /// Both forks contribute at least one class to this concept.
    #[must_use]
    pub fn shared(&self) -> bool {
        !self.redmine.is_empty() && !self.openproject.is_empty()
    }
}

/// The full cross-fork convergence report.
#[derive(Debug, Clone, Deserialize)]
pub struct ForkConvergence {
    /// Schema version tag (`"fork-convergence/2"` since the engine-walking
    /// extractor landed; `"fork-convergence/1"` was the core-only walk).
    pub schema_version: String,
    /// The two forks compared (`["redmine", "openproject"]`).
    pub forks: Vec<String>,
    /// Human-readable lineage chain.
    pub lineage: String,
    /// Total classes extracted from Redmine.
    pub redmine_total: usize,
    /// Total classes extracted from OpenProject (core `app/models` +
    /// every `modules/*/app/models` engine, since `fork-convergence/2`).
    pub openproject_total: usize,
    /// Count of concepts both forks contribute to.
    pub shared_concepts: usize,
    /// Every promoted concept seen in either fork.
    pub concepts: Vec<ConvergedConcept>,
}

impl ForkConvergence {
    /// Parse the embedded convergence artifact.
    #[must_use]
    pub fn load() -> Self {
        serde_json::from_str(FORK_CONVERGENCE_JSON)
            .expect("embedded fork_convergence.json is valid")
    }

    /// Find a concept by name.
    #[must_use]
    pub fn concept(&self, name: &str) -> Option<&ConvergedConcept> {
        self.concepts.iter().find(|c| c.concept == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_forks_mint_the_same_id_for_shared_concepts() {
        // The convergence thesis in code: every concept present in BOTH
        // Redmine and OpenProject carries one project-domain codebook id.
        let fc = ForkConvergence::load();
        let shared: Vec<&ConvergedConcept> = fc.concepts.iter().filter(|c| c.shared()).collect();
        assert_eq!(shared.len(), fc.shared_concepts);
        assert!(shared.len() >= 26, "expected the fork overlap to be broad");
        for c in &shared {
            assert_eq!(
                c.class_id_le[1], 0x01,
                "{} is not in the project-mgmt domain",
                c.concept
            );
            assert_eq!(format!("0x{:04X}", c.class_id_u16()), c.class_id);
        }
    }

    #[test]
    fn divergent_curator_names_converge_on_one_concept() {
        // The headline: the Rails names DIVERGED across the fork, the
        // canonical concept did not. Same id, different curator words.
        let fc = ForkConvergence::load();
        for (concept, redmine_name, op_name) in [
            ("project_work_item", "Issue", "WorkPackage"),
            ("project_status", "IssueStatus", "Status"),
            ("project_type", "Tracker", "Type"),
            ("project_forum", "Board", "Forum"),
        ] {
            let c = fc
                .concept(concept)
                .unwrap_or_else(|| panic!("{concept} missing"));
            assert!(
                c.redmine.contains(&redmine_name.to_string()),
                "{concept}: redmine should carry {redmine_name}"
            );
            assert!(
                c.openproject.contains(&op_name.to_string()),
                "{concept}: openproject should carry {op_name}"
            );
            assert_ne!(redmine_name, op_name, "the point is the names differ");
        }
    }

    #[test]
    fn structural_completers_converge_across_both_forks() {
        // OGAR #72 + #73 — the actor/auth concept and the three structural
        // completers all converge across both forks. Their canonical Rails
        // names (Role, MemberRole, CustomValue, EnabledModule) appear in
        // BOTH curators, even if a fork also ships extra specialized
        // subclasses on top (OpenProject ships both `Role` AND `ProjectRole`,
        // for instance — both collapse into project_role).
        let fc = ForkConvergence::load();
        for (concept, canonical_name) in [
            ("project_role", "Role"),
            ("project_member_role", "MemberRole"),
            ("project_custom_value", "CustomValue"),
            ("project_enabled_module", "EnabledModule"),
        ] {
            let c = fc
                .concept(concept)
                .unwrap_or_else(|| panic!("{concept} missing from convergence artifact"));
            assert!(c.shared(), "{concept} must be contributed by both forks");
            for (side, names) in [("redmine", &c.redmine), ("openproject", &c.openproject)] {
                assert!(
                    names.contains(&canonical_name.to_string()),
                    "{concept}: {side} should carry {canonical_name} (saw {names:?})",
                );
            }
        }
    }

    #[test]
    fn billable_work_entry_bridge_is_complete_both_forks() {
        // The modular extraction gap is CLOSED (ruff#28 + OGAR#75
        // extract_app_with): OpenProject's `TimeEntry` lives in
        // modules/costs/app/models and is now harvested. Both forks ship a
        // class literally named `TimeEntry` that lifts onto the same
        // canonical concept and id — a clean, name-identical convergence on
        // the first cross-domain bridge (the concept Odoo's
        // account.analytic.line also lands on in the commerce arm).
        let fc = ForkConvergence::load();
        let bridge = fc.concept("billable_work_entry").unwrap();
        assert!(bridge.redmine.contains(&"TimeEntry".to_string()));
        assert!(
            bridge.openproject.contains(&"TimeEntry".to_string()),
            "OpenProject TimeEntry must be harvested (extract_app_with)"
        );
        assert!(
            bridge.shared(),
            "billable_work_entry must be shared by both forks"
        );
        assert_eq!(bridge.class_id_u16(), 0x0103);
    }

    #[test]
    fn every_redmine_concept_has_an_openproject_witness() {
        // The headline: with engine-walking, EVERY canonical concept the
        // Redmine corpus contributes is also contributed by OpenProject.
        // Convergence is total at the project-mgmt level — no Redmine-only
        // promoted concept remains.
        let fc = ForkConvergence::load();
        for c in &fc.concepts {
            if !c.redmine.is_empty() {
                assert!(
                    !c.openproject.is_empty(),
                    "{} has Redmine witness but no OpenProject witness ({:?})",
                    c.concept,
                    c.redmine,
                );
            }
        }
        // Same statement in counts: every concept-with-a-Redmine-side has an
        // OpenProject side too.
        let redmine_concepts = fc.concepts.iter().filter(|c| !c.redmine.is_empty()).count();
        assert_eq!(redmine_concepts, fc.shared_concepts);
    }
}
