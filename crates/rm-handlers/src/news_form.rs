//! **W6a D1** — News (`project_news`) create form.
//!
//! Third form module after [`crate::issues_form`] + [`crate::projects_form`]
//! — the one that triggered lifting the shared `render_errors` block into
//! [`crate::common`] (Plan §1.6 "three points form a line").
//!
//! - `GET /news/new` — render an empty form
//! - `POST /news` — validate, create, **303 See Other** to `/news/<id>` on
//!   success (News is record-keyed like Issue — Redmine's `/news/:id` uses
//!   the numeric primary key, no slug); re-render with inline errors on
//!   failure, input preserved.
//!
//! # Today's scope
//!
//! Mirrors Redmine's `News` validations: **title required** (trimmed,
//! ≤255) and **description required**; **summary optional** (≤255). No
//! markdown preview / attachments (separate D-tracks), no CSRF token yet
//! (`csrf_token: ""`, same seam as the sibling forms).

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use ogar_render_askama::{
    render_form, ColumnKind, FormFieldSource, FormSource, InputData, RenderColumn,
};
use rm_store::NewNews;
use serde::Deserialize;
use surrealdb_types::ToSql;

use crate::common::{render_errors, wrap_in_doc, AppState, HandlerError};

/// The submit URL — also the action the GET form points to.
const SUBMIT_PATH: &str = "/news";

/// Max title length — Redmine's `news.title` column cap.
const TITLE_MAX_LEN: usize = 255;

/// Max summary length — Redmine's `news.summary` column cap.
const SUMMARY_MAX_LEN: usize = 255;

/// Form submission shape — Axum's `Form<T>` deserializes the
/// `application/x-www-form-urlencoded` body into this.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewsForm {
    /// Submitted headline. Required (after trimming), ≤255.
    #[serde(default)]
    pub title: String,
    /// Submitted one-line summary. Optional, ≤255.
    #[serde(default)]
    pub summary: String,
    /// Submitted long-form body. Required (after trimming).
    #[serde(default)]
    pub description: String,
}

/// `GET /news/new` — render an empty create form.
pub async fn new_form(State(_state): State<AppState>) -> Result<Html<String>, HandlerError> {
    render(&NewsForm::default(), &[])
}

/// `POST /news` — accept submission, validate, persist, redirect.
///
/// `303 See Other` → `/news/<id>` on success (POST-redirect-GET); `200 OK`
/// re-rendering the form with inline errors on validation failure,
/// preserving the user's input.
pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<NewsForm>,
) -> Result<Response, HandlerError> {
    let errors = validate(&form);
    if !errors.is_empty() {
        let html = render(&form, &errors)?;
        return Ok((StatusCode::OK, html).into_response());
    }
    let new = NewNews {
        title: form.title.trim().to_string(),
        summary: form.summary.trim().to_string(),
        description: form.description.trim().to_string(),
    };
    let entry = state.store.create_news(new).await?;
    let rid = entry
        .id
        .ok_or(HandlerError::Store(rm_store::StoreError::NotFound))?;
    Ok(Redirect::to(&format!("/news/{}", rid.key.to_sql())).into_response())
}

/// Validate the form. Returns human-readable error strings — empty when
/// acceptable. Pure so the unit tests hit every branch without axum.
fn validate(form: &NewsForm) -> Vec<&'static str> {
    let mut errs: Vec<&'static str> = Vec::new();

    let title = form.title.trim();
    if title.is_empty() {
        errs.push("Title is required.");
    }
    if title.chars().count() > TITLE_MAX_LEN {
        errs.push("Title must be 255 characters or fewer.");
    }

    // Summary is optional; only length-cap it when present.
    if form.summary.trim().chars().count() > SUMMARY_MAX_LEN {
        errs.push("Summary must be 255 characters or fewer.");
    }

    if form.description.trim().is_empty() {
        errs.push("Description is required.");
    }
    errs
}

/// Render the create form, optionally with validation errors above the
/// fields. Pure helper around the kit.
fn render(form: &NewsForm, errors: &[&str]) -> Result<Html<String>, HandlerError> {
    let cols = form_columns();
    let fields = vec![
        FormFieldSource {
            column: &cols[0],
            css_classes: "",
            hint: "Headline shown on the news list. Required.",
            data: InputData::Text {
                value: form.title.clone(),
                required: true,
                placeholder: String::new(),
            },
        },
        FormFieldSource {
            column: &cols[1],
            css_classes: "",
            hint: "One-line teaser shown next to the title. Optional.",
            data: InputData::Text {
                value: form.summary.clone(),
                required: false,
                placeholder: String::new(),
            },
        },
        FormFieldSource {
            column: &cols[2],
            css_classes: "",
            hint: "Long-form announcement body. Required.",
            data: InputData::TextArea {
                value: form.description.clone(),
                required: true,
                rows: 8,
                placeholder: String::new(),
            },
        },
    ];
    let src = FormSource {
        method: "post",
        action: SUBMIT_PATH,
        csrf_token: "", // rm-auth integration is its own follow-on
        record_id: None,
        legend: "Add news",
        submit_label: "Create",
        cancel_label: "Cancel",
        cancel_href: SUBMIT_PATH,
        fields,
    };
    let form_html = render_form(0x0114, "project_news", &src)
        .map_err(|e| HandlerError::Render(e.to_string()))?;
    let errors_html = render_errors(errors);
    let body = format!("{errors_html}{form_html}");
    Ok(Html(wrap_in_doc("Add news", &body)))
}

/// Form columns: Title (primary) + Summary (plain) + Description
/// (long-form). The kit reads `required` from each field's `InputData`.
fn form_columns() -> [RenderColumn; 3] {
    [
        RenderColumn::new("title", "Title", ColumnKind::PrimaryLink),
        RenderColumn::new("summary", "Summary", ColumnKind::Plain),
        RenderColumn::new("description", "Description", ColumnKind::Plain),
    ]
}

/// Build the D1 form router (`GET /news/new` + `POST /news`) that
/// `rm-server` merges alongside `news::router`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/news/new", get(new_form))
        .route("/news", post(create))
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
    async fn get_new_renders_empty_form_with_all_three_fields() {
        let res = app()
            .await
            .oneshot(
                Request::builder()
                    .uri("/news/new")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = String::from_utf8(body.to_vec()).unwrap();
        assert!(s.contains(r#"action="/news""#), "form action:\n{s}");
        assert!(s.contains(r#"method="post""#), "form method:\n{s}");
        assert!(s.contains(r#"name="title""#), "title field:\n{s}");
        assert!(s.contains(r#"name="summary""#), "summary field:\n{s}");
        assert!(
            s.contains(r#"name="description""#),
            "description field:\n{s}"
        );
        assert!(
            s.contains(r#"data-class-id="0x0114""#),
            "class id stamped:\n{s}"
        );
        assert!(!s.contains("form-errors"), "no errors on empty form:\n{s}");
    }

    #[tokio::test]
    async fn post_with_valid_fields_creates_and_redirects() {
        let app = app().await;
        let (status, location, _) = submit(
            app,
            "title=Release+0.1&summary=MVP+ships&description=Browse+create+auth",
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER, "must 303 on success");
        let loc = location.expect("Location header");
        assert!(loc.starts_with("/news/"), "expected /news/<key>, got {loc}");
    }

    #[tokio::test]
    async fn post_missing_title_re_renders_with_error() {
        let app = app().await;
        let (status, _, s) = submit(app, "title=&summary=&description=Body").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "validation failure must not redirect"
        );
        assert!(s.contains("Title is required"), "title error:\n{s}");
        assert!(s.contains("form-errors"), "error block:\n{s}");
    }

    #[tokio::test]
    async fn post_missing_description_re_renders_with_error() {
        let app = app().await;
        let (status, _, s) = submit(app, "title=Has+title&summary=&description=").await;
        assert_eq!(status, StatusCode::OK);
        assert!(s.contains("Description is required"), "desc error:\n{s}");
    }

    #[test]
    fn validate_accepts_title_and_description_with_optional_summary() {
        let f = NewsForm {
            title: "Release 0.1".into(),
            summary: String::new(), // optional — empty is fine
            description: "Body".into(),
        };
        assert!(validate(&f).is_empty());
    }

    #[test]
    fn validate_requires_title_and_description() {
        let f = NewsForm {
            title: "  ".into(),
            summary: String::new(),
            description: "   ".into(),
        };
        let errs = validate(&f);
        assert!(errs.contains(&"Title is required."));
        assert!(errs.contains(&"Description is required."));
    }

    #[test]
    fn validate_caps_title_and_summary_lengths() {
        let f = NewsForm {
            title: "a".repeat(TITLE_MAX_LEN + 1),
            summary: "b".repeat(SUMMARY_MAX_LEN + 1),
            description: "ok".into(),
        };
        let errs = validate(&f);
        assert!(errs.contains(&"Title must be 255 characters or fewer."));
        assert!(errs.contains(&"Summary must be 255 characters or fewer."));
    }
}
