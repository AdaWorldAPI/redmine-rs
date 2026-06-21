//! **W7** — SCM-light: Repository + Changeset browse handlers.
//!
//! Read-only. Renders metadata Redmine stores ABOUT repos / commits;
//! no live VCS driver (that's a later sprint per the Integration Plan
//! — "W7 SCM-light (read-only; no Git driver yet)").
//!
//! Routes (mounted by [`router`]):
//!
//! - `GET /repositories`           + `/repositories/:id`        (0x010A)
//! - `GET /changesets`             + `/changesets/:revision`    (0x0112)

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::{ChangesetRow, RepositoryRow};
use surrealdb_types::{RecordId, ToSql};

use crate::common::{
    encode_path_segment, html_escape, identifier_to_u64, record_id_to_u64, wrap_in_doc, AppState,
    HandlerError,
};

// ── Repository (0x010A) ─────────────────────────────────────────────

/// `GET /repositories` — render the repository list.
pub async fn repository_list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let repos = state.store.list_repositories().await?;
    let cols = repository_list_columns();
    let hrefs: Vec<String> = repos
        .iter()
        .map(|r| {
            r.id.as_ref()
                .map(|rid| format!("/repositories/{}", rid.key.to_sql()))
                .unwrap_or_default()
        })
        .collect();
    let ids: Vec<u64> = repos
        .iter()
        .map(|r| r.id.as_ref().map(record_id_to_u64).unwrap_or(0))
        .collect();
    let rows: Vec<RowSource<'_>> = repos
        .iter()
        .enumerate()
        .map(|(idx, r)| RowSource {
            record_id: ids[idx],
            css_classes: "repository",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &r.url,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "",
                    data: CellData::Plain { value: &r.scm_type },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list(
        "Repositories",
        0x010A,
        "project_repository",
        &cols,
        &[],
        &rows,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Repositories", &body)))
}

/// `GET /repositories/:id` — render a repository detail page.
pub async fn repository_detail(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let rid = RecordId::new("repository", id_str.as_str());
    let repo: RepositoryRow = state.store.find_repository(&rid).await?;
    let cols = repository_detail_columns();
    let href = format!("/repositories/{}", id_str);
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&repo.url)
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &repo.url,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "",
            data: CellData::Plain {
                value: &repo.scm_type,
            },
        },
    ];
    let body = render_detail(
        0x010A,
        "project_repository",
        record_id_to_u64(&rid),
        &headline,
        &repo.scm_type,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(&repo.url, &body)))
}

fn repository_list_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("url", "URL", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("scm_type", "SCM", ColumnKind::Plain).sortable(),
    ]
}

fn repository_detail_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("url", "URL", ColumnKind::PrimaryLink),
        RenderColumn::new("scm_type", "SCM", ColumnKind::Plain),
    ]
}

// ── Changeset (0x0112) ──────────────────────────────────────────────

/// `GET /changesets` — render the changeset list.
pub async fn changeset_list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let changesets = state.store.list_changesets().await?;
    let cols = changeset_list_columns();
    let hrefs: Vec<String> = changesets
        .iter()
        .map(|c| format!("/changesets/{}", encode_path_segment(&c.revision)))
        .collect();
    let ids: Vec<u64> = changesets
        .iter()
        .map(|c| identifier_to_u64(&c.revision))
        .collect();
    let rows: Vec<RowSource<'_>> = changesets
        .iter()
        .enumerate()
        .map(|(idx, c)| RowSource {
            record_id: ids[idx],
            css_classes: "changeset",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &c.revision,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "",
                    data: CellData::Plain {
                        value: &c.commit_date,
                    },
                },
                CellSource {
                    column: &cols[2],
                    css_classes: "",
                    data: CellData::Plain { value: &c.comments },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Changesets", 0x0112, "project_changeset", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Changesets", &body)))
}

/// `GET /changesets/:revision` — render a changeset detail page.
pub async fn changeset_detail(
    State(state): State<AppState>,
    Path(revision): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let cs: ChangesetRow = state.store.find_changeset_by_revision(&revision).await?;
    let cols = changeset_detail_columns();
    let href = format!("/changesets/{}", encode_path_segment(&cs.revision));
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&cs.revision)
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &cs.revision,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "",
            data: CellData::Plain {
                value: &cs.commit_date,
            },
        },
        CellSource {
            column: &cols[2],
            css_classes: "",
            data: CellData::Plain {
                value: &cs.comments,
            },
        },
    ];
    let body = render_detail(
        0x0112,
        "project_changeset",
        identifier_to_u64(&cs.revision),
        &headline,
        &cs.commit_date,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(&cs.revision, &body)))
}

fn changeset_list_columns() -> [RenderColumn; 3] {
    [
        RenderColumn::new("revision", "Revision", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("commit_date", "Date", ColumnKind::Plain).sortable(),
        RenderColumn::new("comments", "Comment", ColumnKind::Plain),
    ]
}

fn changeset_detail_columns() -> [RenderColumn; 3] {
    [
        RenderColumn::new("revision", "Revision", ColumnKind::PrimaryLink),
        RenderColumn::new("commit_date", "Date", ColumnKind::Plain),
        RenderColumn::new("comments", "Comment", ColumnKind::Plain),
    ]
}

// ── Router ──────────────────────────────────────────────────────────

/// Build the SCM-light router (Repository + Changeset). One merge
/// call in rm-server brings both.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/repositories", get(repository_list))
        .route("/repositories/:id", get(repository_detail))
        .route("/changesets", get(changeset_list))
        .route("/changesets/:revision", get(changeset_detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewChangeset, NewRepository, Store};
    use tower::ServiceExt;

    async fn body_of(app: Router, uri: &str) -> (StatusCode, String) {
        let res = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn repository_list_empty_state() {
        let store = Store::open().await.unwrap();
        let (status, s) = body_of(router(AppState { store }), "/repositories").await;
        assert_eq!(status, StatusCode::OK);
        assert!(s.contains("data-class-id=\"0x010A\""), "{s}");
        assert!(s.contains("No data."), "{s}");
    }

    #[tokio::test]
    async fn repository_list_and_detail() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_repository(NewRepository {
                url: "https://example.com/repo.git".to_string(),
                scm_type: "Git".to_string(),
            })
            .await
            .unwrap();
        let key = inserted.id.unwrap().key.to_sql();
        let app = router(AppState { store });

        let (_, list) = body_of(app.clone(), "/repositories").await;
        assert!(list.contains("https://example.com/repo.git"), "{list}");
        assert!(list.contains("Git"), "{list}");
        assert!(list.contains("href=\"/repositories/"), "{list}");

        let (status, detail) = body_of(app, &format!("/repositories/{key}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(detail.contains("https://example.com/repo.git"), "{detail}");
        assert!(detail.contains("data-class-id=\"0x010A\""), "{detail}");
    }

    #[tokio::test]
    async fn repository_detail_404() {
        let store = Store::open().await.unwrap();
        let (status, _) = body_of(router(AppState { store }), "/repositories/missing").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn changeset_list_empty_state() {
        let store = Store::open().await.unwrap();
        let (status, s) = body_of(router(AppState { store }), "/changesets").await;
        assert_eq!(status, StatusCode::OK);
        assert!(s.contains("data-class-id=\"0x0112\""), "{s}");
        assert!(s.contains("No data."), "{s}");
    }

    #[tokio::test]
    async fn changeset_list_and_detail_by_revision() {
        let store = Store::open().await.unwrap();
        store
            .create_changeset(NewChangeset {
                revision: "deadbeef".to_string(),
                commit_date: "2026-06-21".to_string(),
                comments: "Initial commit".to_string(),
            })
            .await
            .unwrap();
        let app = router(AppState { store });

        let (_, list) = body_of(app.clone(), "/changesets").await;
        assert!(list.contains("deadbeef"), "{list}");
        assert!(list.contains("Initial commit"), "{list}");
        assert!(list.contains("href=\"/changesets/deadbeef\""), "{list}");

        let (status, detail) = body_of(app, "/changesets/deadbeef").await;
        assert_eq!(status, StatusCode::OK);
        assert!(detail.contains("deadbeef"), "{detail}");
        assert!(detail.contains("data-class-id=\"0x0112\""), "{detail}");
    }

    #[tokio::test]
    async fn changeset_detail_404() {
        let store = Store::open().await.unwrap();
        let (status, _) = body_of(router(AppState { store }), "/changesets/nope").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
