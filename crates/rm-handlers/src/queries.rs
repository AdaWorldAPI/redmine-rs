//! **W8a** — Query (`project_query` codebook id `0x010D`) saved-view
//! list + detail handlers.
//!
//! Routes:
//! - `GET /queries` — list (Name)
//! - `GET /queries/:name` — detail by name slug

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::QueryRow;

use crate::common::{
    encode_path_segment, html_escape, identifier_to_u64, wrap_in_doc, AppState, HandlerError,
};

/// `GET /queries` — render the saved-query list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let queries = state.store.list_queries().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = queries
        .iter()
        .map(|q| format!("/queries/{}", encode_path_segment(&q.name)))
        .collect();
    let ids: Vec<u64> = queries.iter().map(|q| identifier_to_u64(&q.name)).collect();
    let rows: Vec<RowSource<'_>> = queries
        .iter()
        .enumerate()
        .map(|(idx, q)| RowSource {
            record_id: ids[idx],
            css_classes: "query",
            group: None,
            inline: vec![CellSource {
                column: &cols[0],
                css_classes: "",
                data: CellData::PrimaryLink {
                    label: &q.name,
                    href: &hrefs[idx],
                },
            }],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Queries", 0x010D, "project_query", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Queries", &body)))
}

/// `GET /queries/:name` — render a saved-query detail page.
pub async fn detail(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let q: QueryRow = state.store.find_query_by_name(&name).await?;
    let cols = detail_columns();
    let href = format!("/queries/{}", encode_path_segment(&q.name));
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&q.name)
    );
    let cells = vec![CellSource {
        column: &cols[0],
        css_classes: "",
        data: CellData::PrimaryLink {
            label: &q.name,
            href: &href,
        },
    }];
    let body = render_detail(
        0x010D,
        "project_query",
        identifier_to_u64(&q.name),
        &headline,
        &q.name,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(&q.name, &body)))
}

fn list_columns() -> [RenderColumn; 1] {
    [RenderColumn::new("name", "Name", ColumnKind::PrimaryLink)
        .sortable()
        .frozen()]
}

fn detail_columns() -> [RenderColumn; 1] {
    [RenderColumn::new("name", "Name", ColumnKind::PrimaryLink)]
}

/// Build the Query router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/queries", get(list))
        .route("/queries/:name", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewQuery, Store};
    use tower::ServiceExt;

    #[tokio::test]
    async fn list_empty_state() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/queries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x010D\""), "{s}");
        assert!(s.contains("No data."), "{s}");
    }

    #[tokio::test]
    async fn list_and_detail_by_name() {
        let store = Store::open().await.unwrap();
        store
            .create_query(NewQuery {
                name: "Open bugs".to_string(),
            })
            .await
            .unwrap();
        let app = router(AppState { store });

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/queries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec())
            .unwrap();
        assert!(list.contains("Open bugs"), "{list}");
        // space in the name percent-encodes in the href.
        assert!(
            list.contains("href=\"/queries/Open%20bugs\""),
            "expected encoded href:\n{list}"
        );

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/queries/Open%20bugs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail =
            String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec())
                .unwrap();
        assert!(detail.contains("Open bugs"), "{detail}");
        assert!(detail.contains("data-class-id=\"0x010D\""), "{detail}");
    }

    #[tokio::test]
    async fn detail_404_for_unknown_name() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/queries/Nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
