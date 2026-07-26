use crate::{error::ApiError, response};
use axum::{body::Body, http::Response};

const POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'";
const STYLE: &str = "*{box-sizing:border-box}body{margin:0;background:#f4f6f1;color:#10130f;font:16px system-ui,sans-serif}main{width:min(42rem,calc(100% - 2rem));margin:10vh auto;padding:2rem;border:1px solid #c8cdc5;background:#fff}.brand,.eyebrow{font:700 .75rem ui-monospace,monospace;letter-spacing:.12em}.eyebrow{color:#4c6500;margin-top:3rem}h1{font-size:clamp(2rem,8vw,4rem);line-height:.95}form{display:grid;gap:1rem;margin-top:2rem}input{padding:1rem;border:1px solid #747a70}button{width:max-content;padding:.8rem 1rem;border:0;background:#b6ff00;color:#10130f;font-weight:700}.error{color:#9e251b}";

pub(super) fn login(
    host_label: &str,
    continuation: &str,
    failed: bool,
) -> Result<Response<Body>, ApiError> {
    let error = if failed {
        "<p class=\"error\" role=\"alert\">That sign-in key was not accepted</p>"
    } else {
        ""
    };
    page(&format!(
        "<p class=\"eyebrow\">YARD SIGN IN</p><h1>Open {}</h1><p>Enter your Blob Yard login key to continue.</p>{error}<form method=\"post\" action=\"/account/yard-login\"><input type=\"hidden\" name=\"continuation\" value=\"{}\"><label for=\"login-key\">Sign-in key</label><input id=\"login-key\" name=\"login_key\" type=\"password\" autocomplete=\"current-password\" required><button type=\"submit\">Sign in</button></form>",
        crate::response::escape_html(host_label),
        crate::response::escape_html(continuation),
    ))
}

pub(super) fn invalid_link() -> Result<Response<Body>, ApiError> {
    message(
        "Invalid sign-in link",
        "This sign-in link is not valid or has expired",
    )
}

pub(super) fn access_denied() -> Result<Response<Body>, ApiError> {
    message(
        "Access denied",
        "You do not have access to this Yard, or it does not exist.",
    )
}

fn message(title: &str, description: &str) -> Result<Response<Body>, ApiError> {
    page(&format!(
        "<p class=\"eyebrow\">YARD SIGN IN</p><h1>{}</h1><p>{}</p>",
        crate::response::escape_html(title),
        crate::response::escape_html(description)
    ))
}

fn page(content: &str) -> Result<Response<Body>, ApiError> {
    response::secure_html(
        format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Yard sign in | Blob Yard</title><style>{STYLE}</style></head><body><main><p class=\"brand\">BLOB YARD</p>{content}</main></body></html>"
        ),
        POLICY,
    )
}
