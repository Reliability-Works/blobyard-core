use crate::response;
use axum::{
    body::Body,
    http::{HeaderValue, Response},
};

const POLICY: &str = "default-src 'none'; style-src 'unsafe-inline'; form-action 'self' https: http:; base-uri 'none'; frame-ancestors 'none'";
const STYLE: &str = "*{box-sizing:border-box}body{margin:0;background:#f4f6f1;color:#10130f;font:16px system-ui,sans-serif}main{width:min(42rem,calc(100% - 2rem));margin:10vh auto;padding:2rem;border:1px solid #c8cdc5;background:#fff}.brand,.eyebrow{font:700 .75rem ui-monospace,monospace;letter-spacing:.12em}.eyebrow{color:#4c6500;margin-top:3rem}h1{font-size:clamp(2rem,8vw,4rem);line-height:.95}form{display:grid;gap:1rem;margin-top:2rem}code{display:block;overflow-wrap:anywhere;padding:1rem;background:#eef1eb}button{width:max-content;padding:.8rem 1rem;border:0;background:#b6ff00;color:#10130f;font-weight:700}";

pub(super) fn invitation(
    email: &str,
    token: &str,
    continuation: &str,
    host_label: &str,
) -> Response<Body> {
    page(&format!(
        "<p class=\"eyebrow\">YARD INVITATION</p><h1>Join {}</h1><p>This invitation grants Yard access to {}.</p><form method=\"post\" action=\"/account/yard-invite/accept\"><input type=\"hidden\" name=\"token\" value=\"{}\"><input type=\"hidden\" name=\"continuation\" value=\"{}\"><button type=\"submit\">Accept invitation</button></form>",
        response::escape_html(host_label),
        response::escape_html(email),
        response::escape_html(token),
        response::escape_html(continuation),
    ))
}

pub(super) fn accepted(
    login_key: &str,
    exchange_target: &str,
    exchange_code: &str,
) -> Response<Body> {
    page(&format!(
        "<p class=\"eyebrow\">INVITATION ACCEPTED</p><h1>Save your sign-in key</h1><p>This key is shown once. Store it before continuing.</p><code>{}</code><form method=\"get\" action=\"{}\"><input type=\"hidden\" name=\"code\" value=\"{}\"><label><input type=\"checkbox\" required> I have saved this key</label><button type=\"submit\">Continue to Yard</button></form>",
        response::escape_html(login_key),
        response::escape_html(exchange_target),
        response::escape_html(exchange_code),
    ))
}

pub(super) fn invalid_link() -> Response<Body> {
    page(
        "<p class=\"eyebrow\">YARD INVITATION</p><h1>Invalid invitation</h1><p>This invitation is not valid or has expired.</p>",
    )
}

fn page(content: &str) -> Response<Body> {
    response::secure_html_with_policy(
        format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Yard invitation | Blob Yard</title><style>{STYLE}</style></head><body><main><p class=\"brand\">BLOB YARD</p>{content}</main></body></html>"
        ),
        HeaderValue::from_static(POLICY),
    )
}
