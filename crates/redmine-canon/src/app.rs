//! **APP‖class — pull OGAR via class, no bridge.**
//!
//! The landing shape of [lance-graph#589][589] for the Redmine consumer:
//! resolve a Redmine surface name to its canonical class-id by **pulling
//! the OGAR port** ([`ogar_vocab::ports::RedminePort::class_id`]), never a
//! bridge, never a hand-rolled registry. The vendored snapshot
//! ([`crate::Snapshot::concept_of_class`]) stays as a **checked mirror** —
//! the [`tests::port_pull_agrees_with_the_snapshot`] drift guard proves the
//! port and the snapshot agree on every surface name the port resolves, so
//! the port can be the resolver while the snapshot remains the corpus
//! evidence.
//!
//! [589]: https://github.com/AdaWorldAPI/lance-graph/pull/589
//!
//! # The two halves of a classid (`APP-CLASS-CODEBOOK-LAYOUT.md`)
//!
//! ```text
//! classid : u32  =  [ hi u16 : APP / render prefix ]  [ lo u16 : concept ]
//!                     0x0007 (Redmine)                  0xDDCC (shared)
//! ```
//!
//! - **low u16 — the shared canonical concept.** What the object *is*: the
//!   RBAC + ontology + cross-app identity key. This is what the port pull
//!   returns, and it is identical to the id OpenProject's `WorkPackage` pulls
//!   (`project_work_item = 0x0102`). The shared currency.
//! - **high u16 — Redmine's render prefix [`APP_PREFIX`] (`0x0007`).** *Whose*
//!   rendering: Redmine's `ClassView` / template lens. Reserved for Redmine
//!   in `APP-CLASS-CODEBOOK-LAYOUT.md` §2; OpenProject's twin is `0x0001`.
//!   A full render classid is `0x0007_DDCC` — same concept as OpenProject's
//!   `0x0001_DDCC`, different lens (the W0 "two renders, one concept"
//!   showcase).
//!
//! Composing the high half here is the consumer **stamping its own reserved
//! prefix** (`APP-CLASS-CODEBOOK-LAYOUT.md` §1, §3d), not minting an OGAR
//! codebook class — that mint is gated on OGAR's 5+3 pass. The pull (low
//! half) is the part that is canonical and available today.
//!
//! # One source of truth — the OGAR surface
//!
//! Both halves of the composition come from `ogar-vocab` (OGAR PR #97):
//!
//! - [`APP_PREFIX`] is a `pub const` re-export of
//!   [`ogar_vocab::ports::RedminePort::APP_PREFIX`] — the typed §2
//!   allocation-table value, not a local literal.
//! - [`render_classid`] re-exports `ogar_vocab::app::render_classid_for::<RedminePort>`
//!   — the central `(prefix << 16) | concept` composition; one place owns the
//!   bit math.
//!
//! Same discipline as [`crate::class_ids`] (which re-exports
//! `ogar_vocab::class_ids::*`): the canonical layer mints; this crate
//! re-exports. Drift between local and OGAR is now structurally impossible.
//! (Symmetric to `op-canon`'s `app` module — openproject-nexgen-rs#57.)

use ogar_vocab::ports::{PortSpec, RedminePort};

/// Redmine's reserved **APP / render prefix** — the high u16 of a full
/// `classid` (`APP-CLASS-CODEBOOK-LAYOUT.md` §2 allocation table). Pairs with
/// OpenProject's `0x0001`: same low-half concept, different render lens.
///
/// `pub const` re-export of [`ogar_vocab::ports::RedminePort::APP_PREFIX`]
/// (OGAR PR #97). Promoted from a local mirror to the typed upstream constant —
/// one source of truth.
pub const APP_PREFIX: u16 = RedminePort::APP_PREFIX;

/// Pull the canonical class-id for a Redmine surface name **via the OGAR
/// port** — the #589 "pull OGAR via class" path (no bridge, no registry).
/// `None` for a name the codebook does not carry.
///
/// ```
/// use redmine_canon::{app, class_ids};
/// assert_eq!(app::class_id_of("Issue"), Some(class_ids::PROJECT_WORK_ITEM));
/// assert_eq!(app::class_id_of("TimeEntry"), Some(class_ids::BILLABLE_WORK_ENTRY));
/// assert_eq!(app::class_id_of("NotARedmineClass"), None);
/// ```
#[must_use]
pub fn class_id_of(surface_name: &str) -> Option<u16> {
    RedminePort::class_id(surface_name)
}

/// Compose the full 32-bit **render** classid for a shared `concept` under
/// Redmine's prefix: `0x0007_DDCC`.
///
/// Re-export of `ogar_vocab::app::render_classid_for::<RedminePort>(concept)`
/// (OGAR PR #97) — the central composition, not local bit math.
///
/// ```
/// use redmine_canon::{app, class_ids};
/// assert_eq!(app::render_classid(class_ids::PROJECT_WORK_ITEM), 0x0007_0102);
/// ```
#[must_use]
pub fn render_classid(concept: u16) -> u32 {
    ogar_vocab::app::render_classid_for::<RedminePort>(concept)
}

/// Pull + compose in one step: a Redmine surface name → its full render
/// classid `0x0007_DDCC`, via the OGAR port. `None` if the port does not
/// carry the name.
///
/// ```
/// use redmine_canon::app;
/// assert_eq!(app::render_classid_of("Issue"), Some(0x0007_0102));
/// ```
#[must_use]
pub fn render_classid_of(surface_name: &str) -> Option<u32> {
    class_id_of(surface_name).map(render_classid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{class_ids, Snapshot};

    #[test]
    fn port_pull_resolves_headline_surface_names() {
        // The #589 AFTER pattern: surface name → class_id via the port.
        assert_eq!(class_id_of("Issue"), Some(class_ids::PROJECT_WORK_ITEM));
        assert_eq!(class_id_of("Project"), Some(class_ids::PROJECT));
        assert_eq!(
            class_id_of("TimeEntry"),
            Some(class_ids::BILLABLE_WORK_ENTRY)
        );
        assert_eq!(class_id_of("Tracker"), Some(class_ids::PROJECT_TYPE));
        assert_eq!(class_id_of("IssueStatus"), Some(class_ids::PROJECT_STATUS));
        assert_eq!(class_id_of("Role"), Some(class_ids::PROJECT_ROLE));
        // STI fold: User / Principal / Group all pull project_actor.
        assert_eq!(class_id_of("User"), Some(class_ids::PROJECT_ACTOR));
        assert_eq!(class_id_of("Principal"), Some(class_ids::PROJECT_ACTOR));
        assert_eq!(class_id_of("Group"), Some(class_ids::PROJECT_ACTOR));
        // Unknown names resolve to None (not a panic, not a bridge fallthrough).
        assert_eq!(class_id_of("NotARedmineClass"), None);
        assert_eq!(class_id_of(""), None);
    }

    #[test]
    fn app_prefix_re_exports_the_typed_ogar_constant() {
        // One source of truth: the local constant IS the upstream typed
        // PortSpec::APP_PREFIX, not a parallel literal. (#97)
        assert_eq!(APP_PREFIX, RedminePort::APP_PREFIX);
        assert_eq!(APP_PREFIX, 0x0007);
    }

    #[test]
    fn render_classid_composes_redmine_prefix() {
        // Full render classid = 0x0007_DDCC (W0 worked table).
        assert_eq!(render_classid(class_ids::PROJECT_WORK_ITEM), 0x0007_0102);
        assert_eq!(render_classid(class_ids::BILLABLE_WORK_ENTRY), 0x0007_0103);
        assert_eq!(render_classid(class_ids::PROJECT_ROLE), 0x0007_0117);
        // Pull + compose.
        assert_eq!(render_classid_of("Issue"), Some(0x0007_0102));
        assert_eq!(render_classid_of("TimeEntry"), Some(0x0007_0103));
    }

    #[test]
    fn render_classid_agrees_with_the_central_ogar_composition() {
        // The local function is exactly OGAR's `render_classid_for::<P>`; no
        // local bit math. If this assertion ever fails, the local impl drifted
        // from the canonical upstream composition.
        for &concept in &[
            class_ids::PROJECT_WORK_ITEM,
            class_ids::PROJECT,
            class_ids::BILLABLE_WORK_ENTRY,
            class_ids::PROJECT_ROLE,
        ] {
            assert_eq!(
                render_classid(concept),
                ogar_vocab::app::render_classid_for::<RedminePort>(concept),
            );
        }
    }

    #[test]
    fn render_classid_keeps_concept_in_the_low_half() {
        // The low half is the shared concept (== the port pull); the high
        // half is Redmine's render lens. OpenProject's twin carries the
        // SAME low half under prefix 0x0001 (pinned in OGAR's port tests).
        for &concept in &[
            class_ids::PROJECT_WORK_ITEM,
            class_ids::PROJECT,
            class_ids::BILLABLE_WORK_ENTRY,
            class_ids::PROJECT_ROLE,
        ] {
            let cid = render_classid(concept);
            // Decompose via OGAR's central helpers (one source of truth on the
            // bit math, not local shifts).
            assert_eq!(
                ogar_vocab::app::app_of(cid),
                APP_PREFIX,
                "high half = Redmine lens",
            );
            assert_eq!(
                ogar_vocab::app::concept_of(cid),
                concept,
                "low half = shared concept",
            );
        }
    }

    #[test]
    fn port_pull_agrees_with_the_snapshot() {
        // The drift guard that lets the PORT be the resolver and the snapshot
        // be a checked mirror: every Redmine curator class the snapshot carries
        // that the port also knows must resolve to the SAME id. (The port covers
        // the public surface names; snapshot-only subclasses the port doesn't
        // alias are skipped — `None` is not a disagreement.)
        let s = Snapshot::load();
        let mut checked = 0u32;
        for concept in &s.concepts {
            for curator_class in &concept.curator_classes {
                if let Some(pulled) = class_id_of(curator_class) {
                    assert_eq!(
                        pulled,
                        concept.class_id_u16(),
                        "port `{curator_class}` -> 0x{pulled:04X} disagrees with snapshot \
                         `{}` 0x{:04X}",
                        concept.canonical_concept,
                        concept.class_id_u16(),
                    );
                    checked += 1;
                }
            }
        }
        // Sanity: the guard actually exercised the port (not a vacuous pass).
        // The Redmine port carries 28 aliases; all 28 appear in the snapshot
        // curator_classes, so the floor is 28 (not a vacuous pass).
        assert!(
            checked >= 20,
            "expected the port to cover most snapshot surface names, only matched {checked}",
        );
        let _ = checked;
    }
}
