//! **W6b D1** — WikiPage (`project_wiki_page`) create form.
//!
//! Fourth form module — the create flow for the title-keyed wiki page,
//! mirroring [`crate::projects_form`]'s slug-keyed shape (title doubles as
//! the URL slug, like Redmine's wiki).
//!
//! - `GET /wiki/new` — render an empty form
//! - `POST /wiki` — validate, create, **303 See Other** to
//!   `/wiki/<title>` on success; re-render with inline errors on failure.
//!
//! # Today's scope
//!
//! - **Title required**, trimmed, **≤255** — it's the URL slug, so it
//!   must be present; `encode_path_segment` makes the redirect URL-safe
//!   (Redmine wiki titles allow spaces, which percent-encode fine).
//! - **Body optional** — creating a stub page then filling it in is a
//!   real wiki workflow; an empty page is valid.
//! - **Routing caveat**: `/wiki/new` (this form) shadows a page literally
//!   titled `new` (static route beats `/wiki/:title`). Acceptable for the
//!   flattened MVP routing; nested `/projects/:id/wiki/new` lands with the
//!   project-scoped sidebar (the same W2 follow-up that nests wiki under a
//!   project).
//! - **No versioning / preview / attachments** — separate D-tracks.
//! - **No CSRF token yet** (`csrf_token: ""`, same seam as siblings).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use ogar_render_askama::{
    render_form, ColumnKind, FormFieldSource, FormSource, InputData, RenderColumn,
};
use rm_store::NewWikiPage;
use serde::Deserialize;

use crate::common::{encode_path_segment, render_errors, wrap_in_doc, AppState, HandlerError};

/// The submit URL — also the action the GET form points to.
const SUBMIT_PATH: &str = "/wiki";

/// Max title length — the title is the URL slug; cap it at Redmine's 255.
const TITLE_MAX_LEN: usize = 255;

/// Form submission shape.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct WikiForm {
    /// Submitted page title (doubles as the URL slug). Required, ≤255.
    #[serde(default)]
    pub title: String,
    /// Submitted page body. Optional — a stub page is valid.
    #[serde(default)]
    pub body: String,
}

/// `GET /wiki/new` — render an empty create form.
pub async fn new_form(State(_state): State<AppState>) -> Result<Html<String>, HandlerError> {
    render(&WikiForm::default(), &[])
}

/// `POST /wiki` — accept submission, validate, persist, redirect.
///
/// `303 See Other` → `/wiki/<title>` on success (POST-redirect-GET); `200
/// OK` re-rendering with inline errors on validation failure, input
/// preserved.
pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<WikiForm>,
) -> Result<Response, HandlerError> {
    let errors = validate(&form);
    if !errors.is_empty() {
        let html = render(&form, &errors)?;
        return Ok((StatusCode::OK, html).into_response());
    }
    let new = NewWikiPage {
        title: form.title.trim().to_string(),
        body: form.body.trim().to_string(),
    };
    let page = state.store.create_wiki_page(new).await?;
    Ok(Redirect::to(&format!("/wiki/{}", encode_path_segment(&page.title))).into_response())
}

/// Validate the form. Pure so the unit tests hit every branch without axum.
fn validate(form: &WikiForm) -> Vec<&'static str> {
    let mut errs: Vec<&'static str> = Vec::new();
    let title = form.title.trim();
    if title.is_empty() {
        errs.push("Title is required.");
    }
    if title.chars().count() > TITLE_MAX_LEN {
        errs.push("Title must be 255 characters or fewer.");
    }
    errs
}

/// Render the create form, optionally with validation errors above the
/// fields. Pure helper around the kit.
fn render(form: &WikiForm, errors: &[&str]) -> Result<Html<String>, HandlerError> {
    let cols = form_columns();
    let fields = vec![
        FormFieldSource {
            column: &cols[0],
            css_classes: "",
            hint: "Page title — also the URL slug. Required.",
            data: InputData::Text {
                value: form.title.clone(),
                required: true,
                placeholder: String::new(),
            },
        },
        FormFieldSource {
            column: &cols[1],
            css_classes: "",
            hint: "Page content (wiki markup). Optional — you can fill it in later.",
            data: InputData::TextArea {
                value: form.body.clone(),
                required: false,
                rows: 12,
                placeholder: String::new(),
            },
        },
    ];
    let src = FormSource {
        method: "post",
        action: SUBMIT_PATH,
        csrf_token: "", // rm-auth integration is its own follow-on
        record_id: None,
        legend: "New wiki page",
        submit_label: "Create",
        cancel_label: "Cancel",
        cancel_href: SUBMIT_PATH,
        fields,
    };
    let form_html = render_form(0x010C, "project_wiki_page", &src)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    let errors_html = render_errors(errors);
    let body = format!("{errors_html}{form_html}");
    Ok(Html(wrap_in_doc("New wiki page", &body)))
}

/// Form columns: Title (primary) + Body (long-form). The kit reads
/// `required` from each field's `InputData`.
fn form_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("title", "Title", ColumnKind::PrimaryLink),
        RenderColumn::new("body", "Body", ColumnKind::Plain),
    ]
}

/// Build the D1 form router (`GET /wiki/new` + `POST /wiki`) that
/// `rm-server` merges alongside `wiki_pages::router`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/wiki/new", get(new_form))
        .route("/wiki", post(create))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use http_body_util::BodyExt;
    use rm_store::Store;
    use tower::ServiceExt;

    async fn app() -> Router {
        let store = Store::open().await.expect("store boots");
        router(AppState { store })
    }

    async fn submit(app: Router, body: &str) -> (StatusCode, Option<String>, String) {
        let res = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(SUBMIT_PATH)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let location = res
            .headers()
            .get(header::LOCATION)
            .map(|v| v.to_str().unwrap().to_string());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let s = String::from_utf8(bytes.to_vec()).unwrap();
        (status, location, s)
    }

    #[tokio::test]
    async fn get_new_renders_empty_form_with_title_and_body() {
        let res = app()
            .await
            .oneshot(
                Request::builder()
                    .uri("/wiki/new")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = String::from_utf8(body.to_vec()).unwrap();
        assert!(s.contains(r#"action="/wiki""#), "form action:\n{s}");
        assert!(s.contains(r#"name="title""#), "title field:\n{s}");
        assert!(s.contains(r#"name="body""#), "body field:\n{s}");
        assert!(
            s.contains(r#"data-class-id="0x010C""#),
            "class id stamped:\n{s}"
        );
        assert!(!s.contains("form-errors"), "no errors on empty form:\n{s}");
    }

    #[tokio::test]
    async fn post_with_title_creates_and_redirects_to_slug() {
        let app = app().await;
        // Title with a space exercises the URL-encoding of the redirect.
        let (status, location, _) = submit(app, "title=Getting+Started&body=Hello").await;
        assert_eq!(status, StatusCode::SEE_OTHER, "must 303 on success");
        assert_eq!(
            location.as_deref(),
            Some("/wiki/Getting%20Started"),
            "redirect must URL-encode the title slug"
        );
    }

    #[tokio::test]
    async fn post_allows_empty_body_stub_page() {
        let app = app().await;
        let (status, location, _) = submit(app, "title=Stub&body=").await;
        assert_eq!(status, StatusCode::SEE_OTHER, "empty body is a valid stub");
        assert_eq!(location.as_deref(), Some("/wiki/Stub"));
    }

    #[tokio::test]
    async fn post_missing_title_re_renders_with_error() {
        let app = app().await;
        let (status, _, s) = submit(app, "title=&body=Body").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "validation failure must not redirect"
        );
        assert!(s.contains("Title is required"), "title error:\n{s}");
        assert!(s.contains("form-errors"), "error block:\n{s}");
    }

    #[test]
    fn validate_requires_title_only() {
        assert!(validate(&WikiForm {
            title: "Home".into(),
            body: String::new(),
        })
        .is_empty());
        assert_eq!(
            validate(&WikiForm {
                title: "   ".into(),
                body: "x".into(),
            }),
            vec!["Title is required."]
        );
    }

    #[test]
    fn validate_caps_title_length() {
        let errs = validate(&WikiForm {
            title: "a".repeat(TITLE_MAX_LEN + 1),
            body: String::new(),
        });
        assert!(errs.contains(&"Title must be 255 characters or fewer."));
    }
}
