//! **D1** — Project (`project`) create form.
//!
//! The depth follow-on to the W2 `/projects` list/detail on the same
//! surface, mirroring [`crate::issues_form`] exactly one resource over.
//! Redmine's project-creation flow: list page → "New project" → form with
//! Name (required) + Identifier (required, URL slug) → submit → land on the
//! new project's detail page (`/projects/<identifier>`).
//!
//! - `GET /projects/new` — render an empty form
//! - `POST /projects` — validate, create, **303 See Other** to
//!   `/projects/<identifier>` on success; re-render with inline errors on
//!   validation failure.
//!
//! # Today's scope
//!
//! - **Create only** — edit (`PATCH`) is a follow-on, same as issues.
//! - **Name required**, trimmed, **max 255** (Redmine's column cap).
//! - **Identifier required**, trimmed, **max 100**, and constrained to
//!   Redmine's slug rule: starts with a lowercase letter, then lowercase
//!   letters / digits / `-` / `_`. Rejecting at the form boundary surfaces
//!   the rule as a Redmine-shaped validation error instead of a DB failure
//!   (and keeps the redirect target URL-safe).
//! - **No CSRF token yet** (`csrf_token: ""`) — rm-auth's concern, opted
//!   into when that infra is shared (same seam as issues_form).
//! - **No description / homepage / parent / module fields** — those
//!   columns land on `ProjectRow` later; the form grows one field per
//!   column with no chrome change.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use ogar_render_askama::{
    render_form, ColumnKind, FormFieldSource, FormSource, InputData, RenderColumn,
};
use rm_store::NewProject;
use serde::Deserialize;

use crate::common::{encode_path_segment, wrap_in_doc, AppState, HandlerError};

/// The submit URL — also the action the GET form points to.
const SUBMIT_PATH: &str = "/projects";

/// Max project name length — Redmine's `projects.name` column cap.
const NAME_MAX_LEN: usize = 255;

/// Max identifier length — Redmine's `projects.identifier` cap.
const IDENT_MAX_LEN: usize = 100;

/// Form submission shape — Axum's `Form<T>` deserializes the
/// `application/x-www-form-urlencoded` body into this.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectForm {
    /// Submitted display name. Required at validation time (after trimming).
    #[serde(default)]
    pub name: String,
    /// Submitted URL slug. Required + slug-constrained (see [`validate`]).
    #[serde(default)]
    pub identifier: String,
}

/// `GET /projects/new` — render an empty create form.
pub async fn new_form(State(_state): State<AppState>) -> Result<Html<String>, HandlerError> {
    render(&ProjectForm::default(), &[])
}

/// `POST /projects` — accept submission, validate, persist, redirect.
///
/// `303 See Other` → `/projects/<identifier>` on success (POST-redirect-GET
/// so a refresh doesn't re-submit); `200 OK` re-rendering the form with
/// inline errors on validation failure, preserving the user's input.
pub async fn create(
    State(state): State<AppState>,
    Form(form): Form<ProjectForm>,
) -> Result<Response, HandlerError> {
    let errors = validate(&form);
    if !errors.is_empty() {
        let html = render(&form, &errors)?;
        return Ok((StatusCode::OK, html).into_response());
    }
    let new = NewProject {
        name: form.name.trim().to_string(),
        identifier: form.identifier.trim().to_string(),
    };
    let project = state.store.create_project(new).await?;
    // `identifier` is validated URL-safe; `encode_path_segment` is a no-op
    // for the slug charset but keeps the redirect defensive by contract.
    Ok(Redirect::to(&format!(
        "/projects/{}",
        encode_path_segment(&project.identifier)
    ))
    .into_response())
}

/// Validate the form. Returns human-readable error strings — empty when
/// acceptable. Pure so the unit tests hit every branch without axum.
fn validate(form: &ProjectForm) -> Vec<&'static str> {
    let mut errs: Vec<&'static str> = Vec::new();

    let name = form.name.trim();
    if name.is_empty() {
        errs.push("Name is required.");
    }
    if name.chars().count() > NAME_MAX_LEN {
        errs.push("Name must be 255 characters or fewer.");
    }

    // Flat checks (no nested-if) so clippy's collapsible lints stay quiet;
    // the `!is_empty()` guards stop a blank identifier from also tripping
    // the length / charset messages on top of "required".
    let ident = form.identifier.trim();
    if ident.is_empty() {
        errs.push("Identifier is required.");
    }
    if !ident.is_empty() && ident.chars().count() > IDENT_MAX_LEN {
        errs.push("Identifier must be 100 characters or fewer.");
    }
    if !ident.is_empty() && !is_valid_identifier(ident) {
        errs.push(
            "Identifier may contain only lowercase letters, numbers, hyphens and underscores, \
             and must start with a letter.",
        );
    }
    errs
}

/// Redmine's project-identifier slug rule: first char a lowercase ASCII
/// letter, the rest lowercase letters / digits / `-` / `_`. Pure +
/// allocation-free so it's cheap to call and trivially testable.
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Render the create form, optionally with validation errors above the
/// fields. Pure helper around the kit — used by both `GET /projects/new`
/// and the POST validation-failure branch.
fn render(form: &ProjectForm, errors: &[&str]) -> Result<Html<String>, HandlerError> {
    let cols = form_columns();
    let fields = vec![
        FormFieldSource {
            column: &cols[0],
            css_classes: "",
            hint: "Display name shown in lists and navigation. Required.",
            data: InputData::Text {
                value: form.name.clone(),
                required: true,
                placeholder: String::new(),
            },
        },
        FormFieldSource {
            column: &cols[1],
            css_classes: "",
            hint: "URL slug: lowercase letters, numbers, hyphens, underscores; \
                   starts with a letter. Required and unique.",
            data: InputData::Text {
                value: form.identifier.clone(),
                required: true,
                placeholder: String::from("my-project"),
            },
        },
    ];
    let src = FormSource {
        method: "post",
        action: SUBMIT_PATH,
        csrf_token: "", // rm-auth integration is its own follow-on
        record_id: None,
        legend: "New project",
        submit_label: "Create",
        cancel_label: "Cancel",
        cancel_href: SUBMIT_PATH,
        fields,
    };
    let form_html =
        render_form(0x0101, "project", &src).map_err(|e| HandlerError::Render(e.to_string()))?;
    let errors_html = render_errors(errors);
    let body = format!("{errors_html}{form_html}");
    Ok(Html(wrap_in_doc("New project", &body)))
}

/// Render the validation-error block above the form. Empty string when
/// there are no errors. Errors are `&'static str` literals from
/// [`validate`] — no user-controlled content reaches the HTML here.
///
/// Duplicated from [`crate::issues_form`] by the project's "factor on the
/// third caller" rule (Plan §1.6); the third form module (TimeEntry /
/// News) triggers the lift into `common`.
fn render_errors(errors: &[&str]) -> String {
    if errors.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(64 + errors.len() * 32);
    out.push_str(r#"<div class="form-errors" role="alert"><ul>"#);
    for e in errors {
        out.push_str("<li>");
        out.push_str(e);
        out.push_str("</li>");
    }
    out.push_str("</ul></div>");
    out
}

/// Form columns: Name (primary) + Identifier (plain). The kit reads
/// `required` from each field's `InputData`, not the column.
fn form_columns() -> [RenderColumn; 2] {
    [
        RenderColumn::new("name", "Name", ColumnKind::PrimaryLink),
        RenderColumn::new("identifier", "Identifier", ColumnKind::Plain),
    ]
}

/// Build the D1 form router (`GET /projects/new` + `POST /projects`) that
/// `rm-server` merges alongside `projects::router`.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/projects/new", get(new_form))
        .route("/projects", post(create))
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

    // ── GET /projects/new ───────────────────────────────────────────

    #[tokio::test]
    async fn get_new_renders_empty_form_with_name_and_identifier_fields() {
        let res = app()
            .await
            .oneshot(
                Request::builder()
                    .uri("/projects/new")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let s = String::from_utf8(body.to_vec()).unwrap();
        assert!(s.contains(r#"action="/projects""#), "form action:\n{s}");
        assert!(s.contains(r#"method="post""#), "form method:\n{s}");
        assert!(s.contains(r#"name="name""#), "name field:\n{s}");
        assert!(s.contains(r#"name="identifier""#), "identifier field:\n{s}");
        assert!(
            s.contains(r#"data-class-id="0x0101""#),
            "class id stamped:\n{s}"
        );
        assert!(!s.contains("form-errors"), "no errors on empty form:\n{s}");
    }

    // ── POST /projects — success path ───────────────────────────────

    #[tokio::test]
    async fn post_with_valid_fields_creates_and_redirects_to_identifier() {
        let app = app().await;
        let (status, location, _) = submit(app, "name=My+Project&identifier=my-project").await;
        assert_eq!(status, StatusCode::SEE_OTHER, "must 303 on success");
        assert_eq!(
            location.as_deref(),
            Some("/projects/my-project"),
            "redirect must target the new project's slug"
        );
    }

    #[tokio::test]
    async fn post_trims_whitespace_before_persisting() {
        let app = app().await;
        let (status, location, _) = submit(app, "name=++Spaced++&identifier=++spaced++").await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        // Trimmed identifier in the redirect proves the trim happened.
        assert_eq!(location.as_deref(), Some("/projects/spaced"));
    }

    // ── POST /projects — validation-failure paths ───────────────────

    #[tokio::test]
    async fn post_with_empty_name_re_renders_with_error() {
        let app = app().await;
        let (status, _, s) = submit(app, "name=&identifier=ok").await;
        assert_eq!(
            status,
            StatusCode::OK,
            "validation failure must not redirect"
        );
        assert!(s.contains("Name is required"), "name error:\n{s}");
        assert!(s.contains("form-errors"), "error block:\n{s}");
        assert!(
            s.contains(r#"action="/projects""#),
            "form re-rendered:\n{s}"
        );
    }

    #[tokio::test]
    async fn post_with_empty_identifier_re_renders_with_error() {
        let app = app().await;
        let (status, _, s) = submit(app, "name=Has+name&identifier=").await;
        assert_eq!(status, StatusCode::OK);
        assert!(s.contains("Identifier is required"), "ident error:\n{s}");
    }

    #[tokio::test]
    async fn post_with_invalid_identifier_charset_is_rejected() {
        let app = app().await;
        // Uppercase + space + slash are all outside Redmine's slug rule.
        let (status, _, s) = submit(app, "name=Ok&identifier=Bad+Slug%2Fx").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            s.contains("lowercase letters"),
            "charset error must show:\n{s}"
        );
    }

    // ── pure validate() + is_valid_identifier() units ───────────────

    #[test]
    fn validate_accepts_a_normal_project() {
        let f = ProjectForm {
            name: "My Project".into(),
            identifier: "my-project".into(),
        };
        assert!(validate(&f).is_empty());
    }

    #[test]
    fn validate_rejects_blank_name_and_identifier_together() {
        let f = ProjectForm {
            name: "   ".into(),
            identifier: "".into(),
        };
        let errs = validate(&f);
        assert!(errs.contains(&"Name is required."));
        assert!(errs.contains(&"Identifier is required."));
    }

    #[test]
    fn validate_rejects_overlong_name_and_identifier() {
        let f = ProjectForm {
            name: "a".repeat(NAME_MAX_LEN + 1),
            identifier: "a".repeat(IDENT_MAX_LEN + 1),
        };
        let errs = validate(&f);
        assert!(errs.contains(&"Name must be 255 characters or fewer."));
        assert!(errs.contains(&"Identifier must be 100 characters or fewer."));
    }

    #[test]
    fn blank_identifier_does_not_also_trip_charset_or_length() {
        // The `!is_empty()` guards mean a blank identifier yields exactly
        // one identifier error ("required"), not three.
        let f = ProjectForm {
            name: "Ok".into(),
            identifier: "   ".into(),
        };
        let ident_errs: Vec<_> = validate(&f)
            .into_iter()
            .filter(|e| e.starts_with("Identifier"))
            .collect();
        assert_eq!(ident_errs, vec!["Identifier is required."]);
    }

    #[test]
    fn identifier_rule_matches_redmine_slug_shape() {
        // Valid: starts with a letter, then lowercase/digits/-/_.
        for ok in ["a", "my-project", "proj_2", "x1-y2_z3"] {
            assert!(is_valid_identifier(ok), "{ok} should be valid");
        }
        // Invalid: leading digit/hyphen, uppercase, spaces, punctuation.
        for bad in ["1abc", "-abc", "Abc", "a b", "a/b", "a.b", ""] {
            assert!(!is_valid_identifier(bad), "{bad} should be invalid");
        }
    }
}
