//! **W2** — Project (`project` codebook id `0x0101`) list + detail
//! handlers. The container concept Issues / TimeEntries belong to.
//!
//! Routes (mounted by [`router`]):
//!
//! - `GET /projects` — list page, columns `Name` + `Identifier`
//! - `GET /projects/:identifier` — detail page, looked up by the
//!   Redmine-shape slug (matches Redmine's `/projects/<identifier>`
//!   convention every deployment uses)
//!
//! Same shape as W1 (issues.rs); when W3 lands a third resource the
//! common per-resource helpers factor out (Plan §1.6).

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::ProjectRow;

use crate::common::{
    encode_path_segment, html_escape, identifier_to_u64, wrap_in_doc, AppState, HandlerError,
};

/// `GET /projects` — render the project list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let projects = state.store.list_projects().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = projects
        .iter()
        .map(|p| format!("/projects/{}", encode_path_segment(&p.identifier)))
        .collect();
    let ids: Vec<u64> = projects
        .iter()
        .map(|p| identifier_to_u64(&p.identifier))
        .collect();
    let rows: Vec<RowSource<'_>> = projects
        .iter()
        .enumerate()
        .map(|(idx, project)| RowSource {
            record_id: ids[idx],
            css_classes: "project",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &project.name,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "ident",
                    data: CellData::Plain {
                        value: &project.identifier,
                    },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Projects", 0x0101, "project", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Projects", &body)))
}

/// `GET /projects/:identifier` — render a project's detail page.
pub async fn detail(
    State(state): State<AppState>,
    Path(identifier): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let project: ProjectRow = state.store.find_project_by_identifier(&identifier).await?;
    let cols = detail_columns();
    let href = format!("/projects/{}", encode_path_segment(&project.identifier));
    // `project.name` is user-controlled — escape before composing
    // the headline anchor (same XSS guard as W1's issue subject).
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&project.name)
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &project.name,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "ident",
            data: CellData::Plain {
                value: &project.identifier,
            },
        },
    ];
    let body = render_detail(
        0x0101,
        "project",
        identifier_to_u64(&project.identifier),
        &headline,
        &project.identifier,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(
        &format!("{} ({})", &project.name, &project.identifier),
        &body,
    )))
}

/// Project list-view columns.
fn list_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("name", "Name", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("identifier", "Identifier", ColumnKind::Plain).sortable(),
    ]
}

/// Project detail-view columns.
fn detail_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("name", "Name", ColumnKind::PrimaryLink),
        RenderColumn::new("identifier", "Identifier", ColumnKind::Plain),
    ]
}

/// Build the Project router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/projects", get(list))
        .route("/projects/:identifier", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewProject, Store};
    use tower::ServiceExt;

    async fn app_with_projects(seed: &[(&str, &str)]) -> Router {
        let store = Store::open().await.expect("store boots");
        for (name, identifier) in seed {
            store
                .create_project(NewProject {
                    name: name.to_string(),
                    identifier: identifier.to_string(),
                })
                .await
                .expect("seed insert");
        }
        router(AppState { store })
    }

    #[tokio::test]
    async fn list_renders_empty_state_when_no_projects() {
        let app = app_with_projects(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/projects")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(
            s.contains("data-class-id=\"0x0101\""),
            "expected canonical class id 0x0101:\n{s}"
        );
        assert!(s.contains("No data."), "empty-state missing:\n{s}");
    }

    #[tokio::test]
    async fn list_renders_seeded_projects() {
        let app = app_with_projects(&[("Alpha", "alpha"), ("Beta", "beta-proj")]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/projects")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Alpha"), "missing `Alpha`:\n{s}");
        assert!(s.contains("Beta"), "missing `Beta`:\n{s}");
        // Redmine-shape detail href via identifier slug.
        assert!(
            s.contains("href=\"/projects/alpha\""),
            "expected detail href:\n{s}"
        );
        assert!(s.contains("href=\"/projects/beta-proj\""), "{s}");
    }

    #[tokio::test]
    async fn detail_renders_by_identifier() {
        let app = app_with_projects(&[("My Project", "my-proj")]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/projects/my-proj")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("My Project"), "missing name:\n{s}");
        assert!(s.contains("my-proj"), "missing identifier:\n{s}");
        assert!(
            s.contains("data-class-id=\"0x0101\""),
            "expected 0x0101:\n{s}"
        );
    }

    #[tokio::test]
    async fn detail_returns_404_for_unknown_identifier() {
        let app = app_with_projects(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/projects/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
