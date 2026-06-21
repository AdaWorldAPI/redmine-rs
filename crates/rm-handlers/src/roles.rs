//! **W4b** — Role (`project_role` codebook id `0x0117`) list +
//! detail handlers. The RBAC role lookup; D3 (Plan §4 depth) wires
//! the per-permission gates over this same table.
//!
//! Routes (mounted by [`router`]):
//!
//! - `GET /roles` — list page (Name + Position)
//! - `GET /roles/:name` — detail page, looked up by role name slug

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::RoleRow;

use crate::common::{identifier_to_u64, wrap_in_doc, AppState, HandlerError};

/// `GET /roles` — render the role list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let roles = state.store.list_roles().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = roles.iter().map(|r| format!("/roles/{}", r.name)).collect();
    let positions: Vec<String> = roles.iter().map(|r| r.position.to_string()).collect();
    let ids: Vec<u64> = roles.iter().map(|r| identifier_to_u64(&r.name)).collect();
    let rows: Vec<RowSource<'_>> = roles
        .iter()
        .enumerate()
        .map(|(idx, role)| RowSource {
            record_id: ids[idx],
            css_classes: "role",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &role.name,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "num",
                    data: CellData::Plain {
                        value: &positions[idx],
                    },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Roles", 0x0117, "project_role", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Roles", &body)))
}

/// `GET /roles/:name` — render a role's detail page.
pub async fn detail(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let role: RoleRow = state.store.find_role_by_name(&name).await?;
    let cols = detail_columns();
    let href = format!("/roles/{}", role.name);
    let position_str = role.position.to_string();
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        href, &role.name
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &role.name,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "num",
            data: CellData::Plain {
                value: &position_str,
            },
        },
    ];
    let body = render_detail(
        0x0117,
        "project_role",
        identifier_to_u64(&role.name),
        &headline,
        &position_str,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(&format!("Role: {}", &role.name), &body)))
}

fn list_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("name", "Name", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("position", "Position", ColumnKind::Plain).sortable(),
    ]
}

fn detail_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("name", "Name", ColumnKind::PrimaryLink),
        RenderColumn::new("position", "Position", ColumnKind::Plain),
    ]
}

/// Build the Role router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/roles", get(list))
        .route("/roles/:name", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewRole, Store};
    use tower::ServiceExt;

    async fn app_with_roles(seed: &[(&str, i64)]) -> Router {
        let store = Store::open().await.unwrap();
        for (name, position) in seed {
            store
                .create_role(NewRole {
                    name: name.to_string(),
                    position: *position,
                })
                .await
                .unwrap();
        }
        router(AppState { store })
    }

    #[tokio::test]
    async fn list_renders_empty_state() {
        let app = app_with_roles(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/roles")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x0117\""), "{s}");
        assert!(s.contains("No data."), "{s}");
    }

    #[tokio::test]
    async fn list_renders_seeded_roles() {
        let app = app_with_roles(&[("Manager", 1), ("Developer", 2)]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/roles")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Manager"));
        assert!(s.contains("Developer"));
        assert!(s.contains("href=\"/roles/Manager\""));
        assert!(s.contains("href=\"/roles/Developer\""));
    }

    #[tokio::test]
    async fn detail_by_name_renders() {
        let app = app_with_roles(&[("Manager", 1)]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/roles/Manager")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Manager"));
        assert!(s.contains("data-class-id=\"0x0117\""));
    }

    #[tokio::test]
    async fn detail_404_for_unknown_role() {
        let app = app_with_roles(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/roles/Nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
