//! `/api/v1/subscriptions`: price-alert subscriptions (create, confirm, manage).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tracing::{info, warn};
use utoipa::IntoParams;

use super::error::ApiError;
use super::extract::{ApiJson, ApiQuery};
use super::rate_limit::ClientIp;
use crate::db::subscriptions as subs;
use crate::db::{strains, subscriptions::SubscriberRow};
use crate::domain::{
    ConfirmDto, RuleInputDto, RuleKind, RulesUpdateDto, SubscriptionAcceptedDto,
    SubscriptionCreateDto, SubscriptionDto,
};
use crate::mail::{generate_token, templates};
use crate::notify;
use crate::state::SharedState;

pub const MAX_RULES: usize = 20;
/// `NUMERIC(8,2)` upper bound.
const MAX_THRESHOLD: f64 = 999_999.99;
const MAX_EMAIL_LEN: usize = 254;

/// Syntactic e-mail check (one `@`, non-empty local part, dotted domain, no whitespace).
pub fn validate_email(raw: &str) -> Result<String, ApiError> {
    let email = raw.trim();
    let invalid = || ApiError::bad_request("`email` ist keine gültige E-Mail-Adresse");
    if email.is_empty() || email.len() > MAX_EMAIL_LEN {
        return Err(invalid());
    }
    let (local, domain) = email.split_once('@').ok_or_else(invalid)?;
    if local.is_empty()
        || domain.len() < 3
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains("..")
        || domain.contains('@')
        || email.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(invalid());
    }
    Ok(email.to_owned())
}

/// Validate 1–20 rules and their fields per kind; thresholds are rounded to cents.
pub async fn validate_rules(
    state: &SharedState,
    rules: &[RuleInputDto],
) -> Result<Vec<RuleInputDto>, ApiError> {
    if rules.is_empty() || rules.len() > MAX_RULES {
        return Err(ApiError::bad_request(format!(
            "`rules` muss 1 bis {MAX_RULES} Regeln enthalten"
        )));
    }
    let mut out = Vec::with_capacity(rules.len());
    for (index, rule) in rules.iter().enumerate() {
        let kind = rule.kind;
        let at = format!("Regel {} ({})", index + 1, kind.as_str());
        let strain_id = match (kind.needs_strain(), rule.strain_id) {
            (true, None) => {
                return Err(ApiError::bad_request(format!("{at}: `strain_id` fehlt")));
            }
            (false, Some(_)) => {
                return Err(ApiError::bad_request(format!(
                    "{at}: `strain_id` ist bei dieser Regelart nicht erlaubt"
                )));
            }
            (true, Some(id)) => {
                if id <= 0 || strains::get(&state.pool, id).await?.is_none() {
                    return Err(ApiError::bad_request(format!(
                        "{at}: Sorte {id} nicht gefunden"
                    )));
                }
                Some(id)
            }
            (false, None) => None,
        };
        let threshold = match (kind.needs_threshold(), rule.threshold) {
            (true, None) => {
                return Err(ApiError::bad_request(format!("{at}: `threshold` fehlt")));
            }
            (false, Some(_)) => {
                return Err(ApiError::bad_request(format!(
                    "{at}: `threshold` ist bei dieser Regelart nicht erlaubt"
                )));
            }
            (true, Some(value)) => {
                if !value.is_finite() || value <= 0.0 || value > MAX_THRESHOLD {
                    return Err(ApiError::bad_request(format!(
                        "{at}: `threshold` muss eine positive Zahl bis {MAX_THRESHOLD} sein"
                    )));
                }
                if kind == RuleKind::ThcAbove && value > 100.0 {
                    return Err(ApiError::bad_request(format!(
                        "{at}: `threshold` (THC in %) darf höchstens 100 sein"
                    )));
                }
                Some(crate::domain::round2(value))
            }
            (false, None) => None,
        };
        out.push(RuleInputDto {
            kind,
            strain_id,
            threshold,
        });
    }
    Ok(out)
}

async fn subscription_dto(
    state: &SharedState,
    subscriber: &SubscriberRow,
) -> Result<SubscriptionDto, ApiError> {
    let rules = subs::rules_for(&state.pool, subscriber.id).await?;
    Ok(SubscriptionDto {
        email: subscriber.email.clone(),
        confirmed: subscriber.is_confirmed(),
        rules: rules.into_iter().map(Into::into).collect(),
        created_at: subscriber.created_at,
    })
}

async fn send_confirmation(state: &SharedState, subscriber: &SubscriberRow) {
    let email = templates::confirmation(
        &state.config.public_url,
        &subscriber.email,
        &subscriber.confirm_token,
    );
    if let Err(err) = state.mailer.send(email).await {
        warn!(subscriber_id = subscriber.id, %err, "confirmation e-mail failed");
    }
}

fn accepted() -> Response {
    (
        StatusCode::ACCEPTED,
        Json(SubscriptionAcceptedDto {
            status: "confirmation_sent".into(),
        }),
    )
        .into_response()
}

/// Preisalarm anlegen
///
/// Legt einen Abonnenten mit Regeln an und verschickt die Bestätigungsmail. Existiert die
/// E-Mail-Adresse bereits, werden die Regeln hinzugefügt (bei unbestätigten Abonnenten mit
/// erneuter Bestätigungsmail). Die Antwort ist immer `202`, damit keine E-Mail-Adressen
/// erraten werden können. Ein ausgefülltes Honeypot-Feld `website` führt zu `202` ohne Aktion.
#[utoipa::path(
    post,
    path = "/api/v1/subscriptions",
    tag = "subscriptions",
    request_body = SubscriptionCreateDto,
    responses(
        (status = 202, description = "Angenommen; Bestätigungsmail (falls nötig) verschickt", body = SubscriptionAcceptedDto,
            headers(("Cache-Control" = String, description = "`no-store`"))),
        (status = 400, description = "Ungültige E-Mail oder Regeln", body = crate::api::error::ErrorEnvelopeDto),
        (status = 429, description = "Rate-Limit pro IP überschritten (`SUBSCRIPTION_RATE_LIMIT`)", body = crate::api::error::ErrorEnvelopeDto),
        (status = 503, description = "E-Mail-Versand ist nicht konfiguriert", body = crate::api::error::ErrorEnvelopeDto),
    )
)]
pub async fn create(
    State(state): State<SharedState>,
    ClientIp(ip): ClientIp,
    ApiJson(body): ApiJson<SubscriptionCreateDto>,
) -> Result<Response, ApiError> {
    if !state.config.email_enabled {
        return Err(ApiError::service_unavailable(
            "Preisalarme sind derzeit nicht verfügbar",
        ));
    }
    if body
        .website
        .as_deref()
        .is_some_and(|w| !w.trim().is_empty())
    {
        info!(%ip, "subscription honeypot filled, ignoring");
        return Ok(accepted());
    }
    let email = validate_email(&body.email)?;
    let rules = validate_rules(&state, &body.rules).await?;
    if !state.rate_limiter.check(&ip) {
        return Err(ApiError::too_many_requests());
    }

    let (subscriber, notify) = match subs::find_by_email(&state.pool, &email).await? {
        Some(existing) => {
            subs::add_rules(&state.pool, existing.id, &rules).await?;
            subs::touch(&state.pool, existing.id).await?;
            let unconfirmed = !existing.is_confirmed();
            (existing, unconfirmed)
        }
        None => {
            let created =
                subs::insert(&state.pool, &email, &generate_token(), &generate_token()).await?;
            subs::add_rules(&state.pool, created.id, &rules).await?;
            (created, true)
        }
    };
    info!(
        subscriber_id = subscriber.id,
        rules = rules.len(),
        confirmed = subscriber.is_confirmed(),
        "subscription rules added"
    );
    if notify {
        send_confirmation(&state, &subscriber).await;
    }
    notify::refresh_gauge(&state).await;
    Ok(accepted())
}

/// Preisalarm bestätigen
///
/// Bestätigt die E-Mail-Adresse über den `confirm_token` aus der Bestätigungsmail.
#[utoipa::path(
    post,
    path = "/api/v1/subscriptions/confirm",
    tag = "subscriptions",
    request_body = ConfirmDto,
    responses(
        (status = 200, description = "Abo bestätigt", body = SubscriptionDto),
        (status = 400, description = "Ungültiger Body", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "Unbekanntes Token", body = crate::api::error::ErrorEnvelopeDto),
    )
)]
pub async fn confirm(
    State(state): State<SharedState>,
    ApiJson(body): ApiJson<ConfirmDto>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    let token = body.token.trim();
    let Some(subscriber) = (!token.is_empty())
        .then_some(subs::find_by_confirm_token(&state.pool, token).await?)
        .flatten()
    else {
        return Err(ApiError::not_found("Unbekanntes Token"));
    };
    subs::confirm(&state.pool, subscriber.id).await?;
    let subscriber = subs::get(&state.pool, subscriber.id)
        .await?
        .ok_or_else(|| ApiError::not_found("Unbekanntes Token"))?;
    info!(subscriber_id = subscriber.id, "subscription confirmed");
    notify::refresh_gauge(&state).await;
    Ok(Json(subscription_dto(&state, &subscriber).await?))
}

/// `?token=` of the manage endpoints.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ManageQuery {
    /// `manage_token` aus der Benachrichtigungsmail.
    pub token: String,
}

async fn by_manage_token(state: &SharedState, token: &str) -> Result<SubscriberRow, ApiError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ApiError::not_found("Unbekanntes Token"));
    }
    subs::find_by_manage_token(&state.pool, token)
        .await?
        .ok_or_else(|| ApiError::not_found("Unbekanntes Token"))
}

/// Preisalarm anzeigen
#[utoipa::path(
    get,
    path = "/api/v1/subscriptions/manage",
    tag = "subscriptions",
    params(ManageQuery),
    responses(
        (status = 200, description = "Abo mit Regeln", body = SubscriptionDto),
        (status = 404, description = "Unbekanntes Token", body = crate::api::error::ErrorEnvelopeDto),
    )
)]
pub async fn manage_get(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<ManageQuery>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    let subscriber = by_manage_token(&state, &query.token).await?;
    Ok(Json(subscription_dto(&state, &subscriber).await?))
}

/// Regeln ersetzen
///
/// Ersetzt alle Regeln des Abonnenten (1–20 Regeln; zum Abmelden `DELETE` verwenden).
#[utoipa::path(
    put,
    path = "/api/v1/subscriptions/manage",
    tag = "subscriptions",
    params(ManageQuery),
    request_body = RulesUpdateDto,
    responses(
        (status = 200, description = "Abo mit den neuen Regeln", body = SubscriptionDto),
        (status = 400, description = "Ungültige Regeln", body = crate::api::error::ErrorEnvelopeDto),
        (status = 404, description = "Unbekanntes Token", body = crate::api::error::ErrorEnvelopeDto),
    )
)]
pub async fn manage_put(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<ManageQuery>,
    ApiJson(body): ApiJson<RulesUpdateDto>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    let subscriber = by_manage_token(&state, &query.token).await?;
    let rules = validate_rules(&state, &body.rules).await?;
    let mut tx = state.pool.begin().await?;
    subs::delete_rules(&mut *tx, subscriber.id).await?;
    subs::add_rules(&mut *tx, subscriber.id, &rules).await?;
    subs::touch(&mut *tx, subscriber.id).await?;
    tx.commit().await?;
    info!(
        subscriber_id = subscriber.id,
        rules = rules.len(),
        "subscription rules replaced"
    );
    Ok(Json(subscription_dto(&state, &subscriber).await?))
}

/// Abmelden
///
/// Löscht den Abonnenten samt Regeln und Benachrichtigungen.
#[utoipa::path(
    delete,
    path = "/api/v1/subscriptions/manage",
    tag = "subscriptions",
    params(ManageQuery),
    responses(
        (status = 204, description = "Abgemeldet"),
        (status = 404, description = "Unbekanntes Token", body = crate::api::error::ErrorEnvelopeDto),
    )
)]
pub async fn manage_delete(
    State(state): State<SharedState>,
    ApiQuery(query): ApiQuery<ManageQuery>,
) -> Result<StatusCode, ApiError> {
    let subscriber = by_manage_token(&state, &query.token).await?;
    subs::delete(&state.pool, subscriber.id).await?;
    info!(subscriber_id = subscriber.id, "subscription deleted");
    notify::refresh_gauge(&state).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_validation() {
        assert_eq!(
            validate_email(" Max@Example.org ").unwrap(),
            "Max@Example.org"
        );
        assert_eq!(
            validate_email("a.b+c@sub.example.co").unwrap(),
            "a.b+c@sub.example.co"
        );
        for bad in [
            "",
            "max",
            "max@",
            "@example.org",
            "max@example",
            "max@.example.org",
            "max@example.org.",
            "max@exa..mple.org",
            "max @example.org",
            "max@ex@ample.org",
            "max\n@example.org",
        ] {
            assert!(validate_email(bad).is_err(), "{bad:?} should be rejected");
        }
        let long = format!("{}@example.org", "a".repeat(250));
        assert!(validate_email(&long).is_err());
    }
}
