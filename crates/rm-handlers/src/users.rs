//! **W4a** — User (`project_actor` codebook id `0x0104`) list +
//! detail handlers. Top-level admin view of the actor identity.
//!
//! Routes (mounted by [`router`]):
//!
//! - `GET /users` — list page (Login + Display name)
//! - `GET /users/:login` — detail page, looked up by login slug
//!   (Redmine's `/users/:login` convention)

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::UserRow;

use crate::common::{identifier_to_u64, wrap_in_doc, AppState, HandlerError};

/// `GET /users` — render the user list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let users = state.store.list_users().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = users
        .iter()
        .map(|u| format!("/users/{}", u.login))
        .collect();
    let ids: Vec<u64> = users.iter().map(|u| identifier_to_u64(&u.login)).collect();
    let rows: Vec<RowSource<'_>> = users
        .iter()
        .enumerate()
        .map(|(idx, user)| RowSource {
            record_id: ids[idx],
            css_classes: "user",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &user.login,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "",
                    data: CellData::Plain {
                        value: &user.display_name,
                    },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Users", 0x0104, "project_actor", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Users", &body)))
}

/// `GET /users/:login` — render a user's detail page.
pub async fn detail(
    State(state): State<AppState>,
    Path(login): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let user: UserRow = state.store.find_user_by_login(&login).await?;
    let cols = detail_columns();
    let href = format!("/users/{}", user.login);
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        href, &user.display_name
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &user.login,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "",
            data: CellData::Plain {
                value: &user.display_name,
            },
        },
    ];
    let body = render_detail(
        0x0104,
        "project_actor",
        identifier_to_u64(&user.login),
        &headline,
        &user.login,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(
        &format!("{} ({})", &user.display_name, &user.login),
        &body,
    )))
}

fn list_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("login", "Login", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("display_name", "Name", ColumnKind::Plain).sortable(),
    ]
}

fn detail_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("login", "Login", ColumnKind::PrimaryLink),
        RenderColumn::new("display_name", "Name", ColumnKind::Plain),
    ]
}

/// Build the User router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/users", get(list))
        .route("/users/:login", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewUser, Store};
    use tower::ServiceExt;

    async fn app_with_users(seed: &[(&str, &str)]) -> Router {
        let store = Store::open().await.unwrap();
        for (login, name) in seed {
            store
                .create_user(NewUser {
                    login: login.to_string(),
                    display_name: name.to_string(),
                })
                .await
                .unwrap();
        }
        router(AppState { store })
    }

    #[tokio::test]
    async fn list_renders_empty_state() {
        let app = app_with_users(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x0104\""), "{s}");
        assert!(s.contains("No data."), "{s}");
    }

    #[tokio::test]
    async fn list_renders_seeded_users_with_detail_hrefs() {
        let app = app_with_users(&[("admin", "Admin"), ("jsmith", "John Smith")]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("admin"));
        assert!(s.contains("John Smith"));
        assert!(s.contains("href=\"/users/admin\""));
        assert!(s.contains("href=\"/users/jsmith\""));
    }

    #[tokio::test]
    async fn detail_by_login_renders() {
        let app = app_with_users(&[("jsmith", "John Smith")]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/users/jsmith")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("John Smith"), "{s}");
        assert!(s.contains("data-class-id=\"0x0104\""), "{s}");
    }

    #[tokio::test]
    async fn detail_404_for_unknown_login() {
        let app = app_with_users(&[]).await;
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/users/nobody")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
