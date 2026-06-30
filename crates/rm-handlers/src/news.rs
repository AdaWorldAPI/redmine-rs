//! **W6a** — News (`project_news` codebook id `0x0114`).
//!
//! Routes:
//! - `GET /news` — list (Title + Summary)
//! - `GET /news/:id` — detail (record-keyed URL like W1's Issue;
//!   News doesn't have a slug field — Redmine's `/news/:id` uses
//!   the numeric primary key)

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::NewsRow;
use surrealdb_types::{RecordId, ToSql};

use crate::common::{html_escape, record_id_to_u64, wrap_in_doc, AppState, HandlerError};
use crate::list_chrome::render_action_bar;

/// `GET /news` — render the news list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let news = state.store.list_news().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = news
        .iter()
        .map(|n| {
            n.id.as_ref()
                .map(|rid| format!("/news/{}", rid.key.to_sql()))
                .unwrap_or_default()
        })
        .collect();
    let ids: Vec<u64> = news
        .iter()
        .map(|n| n.id.as_ref().map(record_id_to_u64).unwrap_or(0))
        .collect();
    let rows: Vec<RowSource<'_>> = news
        .iter()
        .enumerate()
        .map(|(idx, n)| RowSource {
            record_id: ids[idx],
            css_classes: "news",
            group: None,
            inline: vec![
                CellSource {
                    column: &cols[0],
                    css_classes: "",
                    data: CellData::PrimaryLink {
                        label: &n.title,
                        href: &hrefs[idx],
                    },
                },
                CellSource {
                    column: &cols[1],
                    css_classes: "",
                    data: CellData::Plain { value: &n.summary },
                },
            ],
            block: Vec::new(),
        })
        .collect();
    let table = render_list("News", 0x0114, "project_news", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    // Redmine's "Add news" contextual action, top-right of the list.
    let action_bar = render_action_bar(&[("Add news", "/news/new")]);
    let body = format!("{action_bar}\n{table}");
    Ok(Html(wrap_in_doc("News", &body)))
}

/// `GET /news/:id` — render a single news entry's detail page.
pub async fn detail(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let rid = RecordId::new("news", id_str.as_str());
    let entry: NewsRow = state.store.find_news(&rid).await?;
    let cols = detail_columns();
    let href = format!("/news/{}", id_str);
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&entry.title)
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &entry.title,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "",
            data: CellData::Plain {
                value: &entry.summary,
            },
        },
        CellSource {
            column: &cols[2],
            css_classes: "",
            data: CellData::Plain {
                value: &entry.description,
            },
        },
    ];
    let body = render_detail(
        0x0114,
        "project_news",
        record_id_to_u64(&rid),
        &headline,
        &entry.summary,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(&entry.title, &body)))
}

fn list_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("title", "Title", ColumnKind::PrimaryLink)
            .sortable()
            .frozen(),
        RenderColumn::new("summary", "Summary", ColumnKind::Plain),
    ]
}

fn detail_columns() -> [RenderColumn; 3] {
    [
        RenderColumn::new("title", "Title", ColumnKind::PrimaryLink),
        RenderColumn::new("summary", "Summary", ColumnKind::Plain),
        RenderColumn::new("description", "Description", ColumnKind::Plain),
    ]
}

/// Build the News router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/news", get(list))
        .route("/news/:id", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewNews, Store};
    use tower::ServiceExt;

    #[tokio::test]
    async fn list_renders_empty_state() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(Request::builder().uri("/news").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x0114\""));
        assert!(s.contains("No data."));
    }

    #[tokio::test]
    async fn list_renders_seeded_news() {
        let store = Store::open().await.unwrap();
        store
            .create_news(NewNews {
                title: "Release 0.1".to_string(),
                summary: "MVP ships".to_string(),
                description: "Browse + create + auth.".to_string(),
            })
            .await
            .unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(Request::builder().uri("/news").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Release 0.1"));
        assert!(s.contains("MVP ships"));
        assert!(s.contains("href=\"/news/"));
    }

    #[tokio::test]
    async fn list_shows_add_news_action_link() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(Request::builder().uri("/news").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(
            s.contains(r#"href="/news/new""#) && s.contains("Add news"),
            "expected an Add news CTA linking to the create form:\n{s}"
        );
    }

    #[tokio::test]
    async fn detail_renders_by_record_key() {
        let store = Store::open().await.unwrap();
        let inserted = store
            .create_news(NewNews {
                title: "Detail target".to_string(),
                summary: "sub".to_string(),
                description: "long body".to_string(),
            })
            .await
            .unwrap();
        let key = inserted.id.unwrap().key.to_sql();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri(format!("/news/{key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Detail target"));
        assert!(s.contains("long body"));
        assert!(s.contains("data-class-id=\"0x0114\""));
    }

    #[tokio::test]
    async fn detail_404_for_unknown_id() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/news/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
