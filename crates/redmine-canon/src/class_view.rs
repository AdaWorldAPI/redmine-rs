//! **Canonical ClassView projection** — re-exported from
//! [`ogar_class_view::OgarClassView`], the single source of truth.
//!
//! Pattern mirror of [`crate::class_ids`]: the bridge from
//! [`ogar_vocab::Class`] onto [`lance_graph_contract::ClassView`] lives
//! in OGAR; this port re-exports it so the projection contract is
//! shared with `op-canon`, and downstream `rm-*` crates have one import
//! for the run-time projection layer.
//!
//! ```
//! use redmine_canon::class_view::{OgarClassView, ClassView, FieldMask};
//!
//! let view = OgarClassView::new();
//! let class_id = redmine_canon::class_ids::PROJECT_WORK_ITEM;
//! let mask = FieldMask::EMPTY.with(0).with(1);
//! let rows = view.render_rows(class_id, mask);
//! # let _ = rows;
//! ```
//!
//! Northstar plan §3, C2. The codebook is minted once in
//! [`AdaWorldAPI/OGAR`](https://github.com/AdaWorldAPI/OGAR); the
//! [`OgarClassView`] adapter that lifts every promoted concept onto the
//! `lance_graph_contract::ClassView` trait lives there too. This port
//! re-exports both, so a Redmine consumer holding `redmine_canon`
//! reaches the projection trait + the impl + the constants through one
//! import path.

pub use lance_graph_contract::class_view::{
    ClassId, ClassProjection, ClassView, FieldMask, RenderRow,
};
pub use lance_graph_contract::ontology::{DisplayTemplate, FieldRef, ObjectView};
pub use ogar_class_view::OgarClassView;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Snapshot;

    #[test]
    fn re_export_loads_the_canonical_class_view() {
        // Sanity: the re-export pulled OgarClassView into scope and it
        // initialises cleanly (no panic).
        let view = OgarClassView::new();
        // The view knows about the same project-mgmt concepts the
        // vendored snapshot pins. Walk a few:
        for id in [
            crate::class_ids::PROJECT_WORK_ITEM,
            crate::class_ids::BILLABLE_WORK_ENTRY,
            crate::class_ids::PROJECT_ROLE,
        ] {
            let n = view.field_count(id);
            assert!(
                n > 0,
                "expected non-empty field set for class id 0x{id:04X}"
            );
        }
    }

    #[test]
    fn render_rows_skips_off_bits_through_the_re_export() {
        // The five-line glue from the Northstar plan §2.3 — exercised
        // through redmine-canon's re-export. SoA row knows its class_id;
        // registry knows its ClassView; render_rows projects the
        // populated subset.
        let view = OgarClassView::new();
        let id = crate::class_ids::PROJECT_WORK_ITEM;

        // Empty mask → no rows.
        let empty = view.render_rows(id, FieldMask::EMPTY);
        assert!(empty.is_empty());

        // Bit 0 set → exactly one row.
        let only_first = FieldMask::EMPTY.with(0);
        let rows = view.render_rows(id, only_first);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn snapshot_ids_resolve_through_the_re_exported_view() {
        // End-to-end pin: every concept the redmine snapshot promotes
        // resolves to a non-empty field set via the re-exported
        // ClassView. This is the projection-layer companion of the
        // `snapshot_concepts_match_re_exported_constants` test in
        // class_ids.
        let view = OgarClassView::new();
        let s = Snapshot::load();
        for c in &s.concepts {
            let id = c.class_id_u16();
            let n = view.field_count(id);
            assert!(
                n > 0,
                "snapshot concept {} (id 0x{:04X}) has no ClassView fields",
                c.canonical_concept,
                id,
            );
        }
    }
}
