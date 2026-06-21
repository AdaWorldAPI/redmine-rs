//! **W6b** — WikiPage (`project_wiki_page` codebook id `0x010C`).
//!
//! Routes:
//! - `GET /wiki` — list (Title)
//! - `GET /wiki/:title` — detail (Redmine convention:
//!   `/projects/:proj/wiki/:title`; we flatten to `/wiki/:title` at
//!   the top level for MVP since multi-project nesting lands as a
//!   W2-followup once Member binds Project ↔ User)

use axum::extract::{Path, State};
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use ogar_render_askama::{
    render_detail, render_list, CellData, CellSource, ColumnKind, RenderColumn, RowSource,
};
use rm_store::WikiPageRow;

use crate::common::{
    encode_path_segment, html_escape, identifier_to_u64, wrap_in_doc, AppState, HandlerError,
};

/// `GET /wiki` — render the wiki-page list.
pub async fn list(State(state): State<AppState>) -> Result<Html<String>, HandlerError> {
    let pages = state.store.list_wiki_pages().await?;
    let cols = list_columns();
    let hrefs: Vec<String> = pages
        .iter()
        .map(|p| format!("/wiki/{}", encode_path_segment(&p.title)))
        .collect();
    let ids: Vec<u64> = pages.iter().map(|p| identifier_to_u64(&p.title)).collect();
    let rows: Vec<RowSource<'_>> = pages
        .iter()
        .enumerate()
        .map(|(idx, p)| RowSource {
            record_id: ids[idx],
            css_classes: "wiki-page",
            group: None,
            inline: vec![CellSource {
                column: &cols[0],
                css_classes: "",
                data: CellData::PrimaryLink {
                    label: &p.title,
                    href: &hrefs[idx],
                },
            }],
            block: Vec::new(),
        })
        .collect();
    let body = render_list("Wiki", 0x010C, "project_wiki_page", &cols, &[], &rows)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc("Wiki", &body)))
}

/// `GET /wiki/:title` — render a wiki-page detail.
pub async fn detail(
    State(state): State<AppState>,
    Path(title): Path<String>,
) -> Result<Html<String>, HandlerError> {
    let page: WikiPageRow = state.store.find_wiki_page_by_title(&title).await?;
    let cols = detail_columns();
    let href = format!("/wiki/{}", encode_path_segment(&page.title));
    let headline = format!(
        "<a href=\"{}\" class=\"primary-link\">{}</a>",
        html_escape(&href),
        html_escape(&page.title)
    );
    let cells = vec![
        CellSource {
            column: &cols[0],
            css_classes: "",
            data: CellData::PrimaryLink {
                label: &page.title,
                href: &href,
            },
        },
        CellSource {
            column: &cols[1],
            css_classes: "",
            data: CellData::Plain { value: &page.body },
        },
    ];
    let body = render_detail(
        0x010C,
        "project_wiki_page",
        identifier_to_u64(&page.title),
        &headline,
        &page.title,
        &cols,
        &cells,
    )
    .map_err(|e| HandlerError::Render(e.to_string()))?;
    Ok(Html(wrap_in_doc(&page.title, &body)))
}

fn list_columns() -> [RenderColumn; 1] {
    [RenderColumn::new("title", "Title", ColumnKind::PrimaryLink)
        .sortable()
        .frozen()]
}

fn detail_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("title", "Title", ColumnKind::PrimaryLink),
        RenderColumn::new("body", "Body", ColumnKind::Plain),
    ]
}

/// Build the WikiPage router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/wiki", get(list))
        .route("/wiki/:title", get(detail))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::{NewWikiPage, Store};
    use tower::ServiceExt;

    #[tokio::test]
    async fn list_renders_empty_state() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(Request::builder().uri("/wiki").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("data-class-id=\"0x010C\""));
        assert!(s.contains("No data."));
    }

    #[tokio::test]
    async fn detail_by_title_renders() {
        let store = Store::open().await.unwrap();
        store
            .create_wiki_page(NewWikiPage {
                title: "Home".to_string(),
                body: "Welcome to the wiki.".to_string(),
            })
            .await
            .unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/wiki/Home")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(s.contains("Home"));
        assert!(s.contains("Welcome to the wiki."));
        assert!(s.contains("data-class-id=\"0x010C\""));
    }

    #[tokio::test]
    async fn detail_404_for_unknown_title() {
        let store = Store::open().await.unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/wiki/Nope")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn list_percent_encodes_titles_with_reserved_chars() {
        let store = Store::open().await.unwrap();
        store
            .create_wiki_page(NewWikiPage {
                title: "A/B testing".to_string(),
                body: String::new(),
            })
            .await
            .unwrap();
        let app = router(AppState { store });
        let res = app
            .oneshot(Request::builder().uri("/wiki").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = std::str::from_utf8(&body).unwrap();
        assert!(
            s.contains("href=\"/wiki/A%2FB%20testing\""),
            "expected percent-encoded title in href:\n{s}"
        );
    }
}
