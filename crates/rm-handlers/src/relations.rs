//! **W8b** — IssueRelation (`project_relation` codebook id `0x0111`)
//! list + detail handlers. Work-item dependency edges.
//!
//! Routes:
//! - `GET /relations` — list (Type + Lag)
//! - `GET /relations/:id` — detail by record key (relations have no
//!   natural slug — they're identified by endpoints + type)

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::RelationRow;
use surrealdb_types::{RecordId, ToSql};

use crate::common::{html_escape, record_id_to_u64, wrap_in_doc, AppState, HandlerError};

/// `GET /relations` — render the relation list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let relations = state.store.list_relations().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = relations
        .iter()
        .map(|r| {
            r.id.as_ref()
                .map(|rid| format!("/relations/{}", rid.key.to_sql()))
                .unwrap_or_default()
        })
        .collect();
    let lags: Vec<String> = relations.iter().map(|r| r.lag.to_string()).collect();
    let ids: Vec<u64> = relations
        .iter()
        .map(|r| r.id.as_ref().map(record_id_to_u64).unwrap_or(0))
        .collect();
    let rows: Vec<RowSource<'_>> = relations
        .iter()
        .enumerate()
        .map(|(idx, r)| RowSource {
            record_id: ids[idx],
            css_classes: "relation",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &r.relation_type,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "num",
                    data: CellData::Plain { value: &lags[idx] },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Relations", 0x0111, "project_relation", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Relations", &body)))
}

/// `GET /relations/:id` — render a relation detail page.
pub async fn detail(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let rid = RecordId::new("relation", id_str.as_str());
    let rel: RelationRow = state.store.find_relation(&rid).await?;
    let cols = detail_columns();
    let href = format!("/relations/{}", id_str);
    let lag_str = rel.lag.to_string();
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&rel.relation_type)
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &rel.relation_type,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "num",
            data: CellData::Plain { value: &lag_str },
        },
    ];
    let body = render_detail(
        0x0111,
        "project_relation",
        record_id_to_u64(&rid),
        &headline,
        &lag_str,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(&rel.relation_type, &body)))
}

fn list_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("relation_type", "Type", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("lag", "Lag (days)", ColumnKind::Plain).sortable(),
    ]
}

fn detail_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("relation_type", "Type", ColumnKind::PrimaryLink),
        RenderColumn::new("lag", "Lag (days)", ColumnKind::Plain),
    ]
}

/// Build the Relation router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/relations", get(list))
        .route("/relations/:id", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewRelation, Store};
    use tower::ServiceExt;

    #[tokio::test]
    async fn list_empty_state() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/relations")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x0111\""), "{s}");
        assert!(s.contains("No data."), "{s}");
    }

    #[tokio::test]
    async fn list_and_detail_by_record_key() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_relation(NewRelation {
                relation_type: "precedes".to_string(),
                lag: 3,
            })
            .await
            .unwrap();
        let key = inserted.id.unwrap().key.to_sql();
        let app = router(AppState { store });

        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/relations")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let list = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec())
            .unwrap();
        assert!(list.contains("precedes"), "{list}");
        assert!(list.contains("href=\"/relations/"), "{list}");

        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/relations/{key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let detail =
            String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec())
                .unwrap();
        assert!(detail.contains("precedes"), "{detail}");
        assert!(detail.contains("data-class-id=\"0x0111\""), "{detail}");
    }

    #[tokio::test]
    async fn detail_404_for_unknown_id() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/relations/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
