//! `redmine-canon` — the **canonical contract** for the Redmine → Rust port.
//!
//! This crate carries the OGAR *canonical extraction snapshot* of the
//! Redmine source corpus (`AdaWorldAPI/redmine`) and exposes it as typed
//! Rust. It is the spine of `redmine-rs`: every future domain crate
//! (`rm-work-items`, `rm-actors`, …) keys off the **codebook ids** pinned
//! here, not off Redmine's Rails class names.
//!
//! # Why a snapshot, not a live extraction
//!
//! The mapping (Redmine class → canonical concept → `u16` codebook id) is
//! produced *upstream*, in `AdaWorldAPI/OGAR`, by the producer pipeline:
//!
//! ```text
//!   AdaWorldAPI/redmine (Ruby)
//!         │  ruff_ruby_spo::extract_with(path, "redmine")
//!         ▼
//!   ruff_spo_triplet::ModelGraph
//!         │  ogar_from_ruff::lift_model_graph   (domain-gated)
//!         ▼
//!   Vec<ogar_vocab::Class>  ──  canonical_concept + canonical_id (CODEBOOK)
//!         │  snapshot dump
//!         ▼
//!   crates/redmine-canon/data/redmine.ogar.json   (this crate vendors it)
//! ```
//!
//! Vendoring the snapshot keeps this crate's tests **self-contained** — CI
//! needs no Ruby corpus and no network — while the snapshot stays the
//! single source of truth a regeneration run overwrites.
//!
//! # The codebook is domain-encoded (`0xDDCC`)
//!
//! Each canonical concept owns a stable `u16` id whose **high byte is its
//! domain**. Redmine is a project-management curator, so every id here
//! lives in the `0x01` (project-mgmt) block. A consumer holding several
//! domains routes on `id >> 8` with no table lookup — see
//! [`Concept::domain_high_byte`]. Ids serialise as 2 little-endian bytes
//! (`class_id_le`), wire-compatible with the OGAR `NodeGuid` layout.
//!
//! "Rails words die, the invariant lives": `Issue` and OpenProject's
//! `WorkPackage` are the *same* node identity (`project_work_item`,
//! `0x0102`) because the fork lineage (Redmine → ChiliProject →
//! OpenProject) preserved the shape.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod convergence;

use serde::Deserialize;

/// The raw vendored snapshot JSON (the single source of truth).
pub const SNAPSHOT_JSON: &str = include_str!("../data/redmine.ogar.json");

/// Provenance of a [`Snapshot`] — where the extraction came from and how.
#[derive(Debug, Clone, Deserialize)]
pub struct Provenance {
    /// The specific curator product (`"redmine"`).
    pub source_curator: String,
    /// The repository the corpus was harvested from.
    pub source_repo: String,
    /// The coarse domain bucket (`"project"`).
    pub source_domain: String,
    /// The producer pipeline that emitted the snapshot.
    pub extractor: String,
    /// The codebook the ids are minted from.
    pub ogar_codebook: String,
    /// ISO date the snapshot was generated.
    pub generated: String,
}

/// One promoted canonical concept that the Redmine corpus exhibits.
#[derive(Debug, Clone, Deserialize)]
pub struct Concept {
    /// Canonical concept name (curator-agnostic, e.g. `project_work_item`).
    pub canonical_concept: String,
    /// Codebook id as a `0xDDCC` hex string (e.g. `"0x0102"`).
    pub class_id: String,
    /// Codebook id as 2 little-endian bytes — the wire form.
    pub class_id_le: [u8; 2],
    /// The Redmine Rails class name(s) that converge onto this concept.
    pub curator_classes: Vec<String>,
}

impl Concept {
    /// The codebook id as a `u16`, decoded from its little-endian bytes.
    #[must_use]
    pub fn class_id_u16(&self) -> u16 {
        u16::from_le_bytes(self.class_id_le)
    }

    /// The domain high byte (`id >> 8`). `0x01` for every Redmine concept.
    #[must_use]
    pub fn domain_high_byte(&self) -> u8 {
        self.class_id_le[1]
    }
}

/// The full canonical snapshot for the Redmine curator.
#[derive(Debug, Clone, Deserialize)]
pub struct Snapshot {
    /// Schema version tag (`"redmine-canon/1"`).
    pub schema_version: String,
    /// Where the snapshot came from.
    pub provenance: Provenance,
    /// Total classes the producer extracted from the corpus.
    pub total_classes_extracted: usize,
    /// How many of those promoted into a codebook concept.
    pub promoted_classes: usize,
    /// The promoted canonical concepts (sorted by concept name).
    pub concepts: Vec<Concept>,
}

impl Snapshot {
    /// Parse the embedded snapshot. Panics only if the vendored JSON is
    /// malformed — which a test in this crate guarantees it is not.
    #[must_use]
    pub fn load() -> Self {
        serde_json::from_str(SNAPSHOT_JSON).expect("embedded redmine.ogar.json is valid JSON")
    }

    /// Find a concept by its canonical name.
    #[must_use]
    pub fn concept(&self, canonical: &str) -> Option<&Concept> {
        self.concepts
            .iter()
            .find(|c| c.canonical_concept == canonical)
    }

    /// Reverse lookup: which canonical concept a Redmine Rails class maps
    /// to (`"Issue"` → `project_work_item`).
    #[must_use]
    pub fn concept_of_class(&self, curator_class: &str) -> Option<&Concept> {
        self.concepts
            .iter()
            .find(|c| c.curator_classes.iter().any(|n| n == curator_class))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_loads_as_redmine_project_domain() {
        let s = Snapshot::load();
        assert_eq!(s.schema_version, "redmine-canon/1");
        assert_eq!(s.provenance.source_curator, "redmine");
        assert_eq!(s.provenance.source_domain, "project");
        assert!(
            s.total_classes_extracted >= 100,
            "corpus shrank unexpectedly"
        );
        assert!(!s.concepts.is_empty());
        // promoted_classes counts curator classes, not concepts (some
        // concepts collapse 2+ Redmine classes, e.g. Principal + User).
        let curator_class_count: usize = s.concepts.iter().map(|c| c.curator_classes.len()).sum();
        assert_eq!(s.promoted_classes, curator_class_count);
    }

    #[test]
    fn every_promoted_id_is_in_the_project_mgmt_domain() {
        // The payoff of domain-encoded ids: a project-domain curator only
        // yields project-mgmt (0x01) codebook ids — no cross-domain leak.
        for c in &Snapshot::load().concepts {
            assert_eq!(
                c.domain_high_byte(),
                0x01,
                "{} ({}) is not in the project-mgmt domain",
                c.canonical_concept,
                c.class_id
            );
            // The hex string and the LE bytes agree.
            assert_eq!(format!("0x{:04X}", c.class_id_u16()), c.class_id);
        }
    }

    #[test]
    fn codebook_ids_are_unique() {
        use std::collections::HashSet;
        let s = Snapshot::load();
        let mut seen = HashSet::new();
        for c in &s.concepts {
            assert!(seen.insert(c.class_id_u16()), "duplicate id {}", c.class_id);
            assert_ne!(
                c.class_id_u16(),
                0,
                "id must be non-zero (0x0000 is reserved)"
            );
        }
    }

    #[test]
    fn fork_lineage_convergence_invariants_hold() {
        // Redmine's class names map onto the canonical concepts OpenProject
        // also lands on. These are the invariants the real-corpus
        // convergence tests in OGAR prove; the snapshot pins them here.
        let s = Snapshot::load();
        for (curator_class, concept, id) in [
            ("Issue", "project_work_item", 0x0102u16),
            ("TimeEntry", "billable_work_entry", 0x0103),
            ("Project", "project", 0x0101),
            ("User", "project_actor", 0x0104),
            ("IssueStatus", "project_status", 0x0105),
            ("Tracker", "project_type", 0x0106),
            ("IssuePriority", "priority", 0x0107),
            ("Board", "project_forum", 0x0116),
        ] {
            let c = s
                .concept_of_class(curator_class)
                .unwrap_or_else(|| panic!("{curator_class} not mapped in the snapshot"));
            assert_eq!(c.canonical_concept, concept, "{curator_class} concept");
            assert_eq!(c.class_id_u16(), id, "{curator_class} id");
        }
    }

    #[test]
    fn project_actor_collapses_the_sti_chain() {
        // Principal (STI root) + User (STI child) are the SAME actor
        // identity — both Redmine classes converge onto one concept/id.
        let s = Snapshot::load();
        let actor = s.concept("project_actor").unwrap();
        assert!(actor.curator_classes.contains(&"User".to_string()));
        assert!(actor.curator_classes.contains(&"Principal".to_string()));
        assert_eq!(actor.class_id_u16(), 0x0104);
    }

    #[test]
    fn billable_work_entry_is_the_cross_domain_bridge() {
        // Redmine TimeEntry -> billable_work_entry, the first convergence
        // invariant: the concept Odoo's account.analytic.line ALSO lands
        // on. Its home id is project-domain (0x0103) but it bridges erp.
        let s = Snapshot::load();
        let bridge = s.concept("billable_work_entry").unwrap();
        assert_eq!(bridge.curator_classes, vec!["TimeEntry".to_string()]);
        assert_eq!(bridge.class_id_u16(), 0x0103);
    }
}
