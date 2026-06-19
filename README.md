# redmine-rs

A Rust port of **Redmine** — the project-management arm of the AdaWorldAPI
canonical-transcoding effort, **grounded in the OGAR canonical codebook**.

Redmine and OpenProject share a fork lineage (Redmine → ChiliProject →
OpenProject), so they converge on the *same* canonical concepts. `redmine-rs`
is the Redmine-side sibling of
[`openproject-nexgen-rs`](https://github.com/AdaWorldAPI/openproject-nexgen-rs):
both Rust ports speak the **same domain-encoded codebook** minted in
[`AdaWorldAPI/OGAR`](https://github.com/AdaWorldAPI/OGAR), so a node typed
`project_work_item` (`0x0102`) means the same thing whether it came from
Redmine's `Issue` or OpenProject's `WorkPackage`.

> "Rails words die, the invariant lives."

## The three repositories

```text
  AdaWorldAPI/redmine            (source — Ruby on Rails, GPL-2.0)
        │
        │  ruff fork pipeline + OGAR canonical layer:
        │   ─ ruff_ruby_spo::extract_with(path, "redmine")   Ruby/Rails frontend
        │   ─ ogar_from_ruff::lift_model_graph               domain-gated lift
        │   ─ ogar_vocab  (CODEBOOK)                          canonical concept + u16 id
        ▼
  AdaWorldAPI/OGAR               (canonical vocab — the codebook is minted here)
        │
        │  [snapshot dump → crates/redmine-canon/data/redmine.ogar.json]
        ▼
  AdaWorldAPI/redmine-rs         (THIS REPO — canonical-grounded Rust target)
```

Unlike `openproject-nexgen-rs` (which was seeded *structure-first* by mirroring
a manual port), `redmine-rs` is seeded **canonical-first**: the foundation is
the OGAR extraction snapshot, and domain crates are layered on top of the
codebook ids it pins.

## Current state — the seed

This repo is a **seed** (see [`SEED.md`](SEED.md)). Today it contains the
canonical contract; the domain crates that build on it come next.

| Crate | Role |
|-------|------|
| `redmine-canon` | The canonical contract. Vendors the OGAR extraction snapshot of the Redmine corpus and exposes it as typed Rust, with tests pinning the fork-lineage convergence invariants. |

### What the snapshot says

The producer pipeline extracted **111 Redmine classes**; **26** of them
promote into **22 canonical concepts**, every id in the `0x01`
(project-management) domain block:

| Redmine class(es)        | Canonical concept       | Codebook id |
|--------------------------|-------------------------|-------------|
| `Project`                | `project`               | `0x0101`    |
| `Issue`                  | `project_work_item`     | `0x0102`    |
| `TimeEntry`              | `billable_work_entry`   | `0x0103` ✦  |
| `Principal`, `User`      | `project_actor`         | `0x0104`    |
| `IssueStatus`            | `project_status`        | `0x0105`    |
| `Tracker`                | `project_type`          | `0x0106`    |
| `IssuePriority`          | `priority`              | `0x0107`    |
| `Member`                 | `project_membership`    | `0x0108`    |
| `Journal`                | `project_journal`       | `0x0109`    |
| `Repository`             | `project_repository`    | `0x010A`    |
| `Version`                | `project_version`       | `0x010B`    |
| `WikiPage`               | `project_wiki_page`     | `0x010C`    |
| `Query`, `ProjectQuery`  | `project_query`         | `0x010D`    |
| `Attachment`             | `project_attachment`    | `0x010E`    |
| `Comment`                | `project_comment`       | `0x010F`    |
| `CustomField`, `ProjectCustomField` | `project_custom_field` | `0x0110` |
| `IssueRelation`, `Relations` | `project_relation`  | `0x0111`    |
| `Changeset`              | `project_changeset`     | `0x0112`    |
| `Watcher`                | `project_watcher`       | `0x0113`    |
| `News`                   | `project_news`          | `0x0114`    |
| `Message`                | `project_message`       | `0x0115`    |
| `Board`                  | `project_forum`         | `0x0116`    |

✦ `billable_work_entry` is a **cross-domain bridge**: Odoo's
`account.analytic.line` (commerce/ERP) converges on the same concept, so the
shared `0x0103` identity is the first convergence invariant.

> The codebook grows as OGAR promotes more concepts (`project_role`,
> `project_member_role`, … are in-flight). The snapshot is regenerated, never
> hand-edited.

## Build

```bash
cargo test          # self-contained: no Ruby corpus or network needed
```

The `redmine-canon` tests assert the snapshot loads, every id is in the
project-mgmt domain, ids are unique and non-zero, and the fork-lineage
convergence invariants hold.

## License

GPL-2.0-or-later, matching upstream Redmine.
