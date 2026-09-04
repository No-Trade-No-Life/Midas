use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use auth_mini_axum::{AuthMiniVerifier, JwksCachePolicy};
use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool, sqlite::SqliteConnectOptions};
use tower_http::{cors::CorsLayer, services::ServeDir};
use uuid::Uuid;

const AUTH_MINI_BASE_URL: &str = "https://auth.ntnl.io";
const AUTH_MINI_AUDIENCE: &str = "midas.ntnl.io";
const ROOT_USER_ID_KEY: &str = "root_user_id";
const GAS_ACCOUNT_ADDRESS_KEY: &str = "evm_gas_account_address";
const COLLECTION_WALLET_ADDRESS_KEY: &str = "evm_collection_wallet_address";

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    verifier: AuthMiniVerifier,
}

#[derive(Debug)]
struct ApiError(StatusCode, &'static str);

impl ApiError {
    fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "authentication is required")
    }
    fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "root access is required")
    }
    fn not_configured() -> Self {
        Self(StatusCode::CONFLICT, "Midas root setup is required")
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    service: &'static str,
    custody_execution: &'static str,
}

#[derive(Serialize)]
struct AuthConfig {
    auth_mini_base_url: &'static str,
    audiences: Vec<&'static str>,
    linkit_base_url: &'static str,
}

#[derive(Serialize)]
struct SetupStatus {
    initialized: bool,
    root_user_id: Option<String>,
}

#[derive(Deserialize)]
struct SetupRequest {
    root_user_id: String,
}

#[derive(Serialize)]
struct Balance {
    currency: &'static str,
    available_usd_micros: i64,
    available_usd: String,
}

#[derive(Serialize)]
struct LedgerEntry {
    id: String,
    kind: String,
    status: String,
    amount_usd_micros: i64,
    balance_delta_usd_micros: i64,
    created_at: String,
    posted_at: Option<String>,
    asset_symbol: Option<String>,
    chain_id: Option<i64>,
    external_reference: Option<String>,
}

#[derive(Serialize)]
struct EvmConfig {
    gas_account_address: Option<String>,
    collection_wallet_address: Option<String>,
    networks: Vec<EvmNetwork>,
    custody_private_key_execution: &'static str,
}

#[derive(Serialize, Deserialize)]
struct EvmNetwork {
    chain_id: i64,
    name: String,
    rpc_url: String,
    enabled: bool,
}

#[derive(Deserialize)]
struct EvmConfigInput {
    gas_account_address: Option<String>,
    collection_wallet_address: Option<String>,
    networks: Vec<EvmNetwork>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = data_dir();
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("midas.sqlite3");
    let db = open_db(&db_path).await?;
    migrate(&db).await?;
    let verifier = AuthMiniVerifier::from_issuer(
        AUTH_MINI_BASE_URL,
        AUTH_MINI_AUDIENCE.to_string(),
        JwksCachePolicy::default(),
    )
    .await
    .context("initialize Auth Mini verifier")?;
    let app = app(AppState { db, verifier });
    let addr = SocketAddr::from(([127, 0, 0, 1], 8787));
    println!("Midas v1 skeleton listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/openapi.yaml", get(openapi_yaml))
        .route("/auth/config", get(auth_config))
        .route("/setup/status", get(setup_status))
        .route("/setup/initialize", post(setup_initialize))
        .route("/balances/me", get(my_balance))
        .route("/ledger/me", get(my_ledger))
        .route("/wallet-addresses/me", get(my_wallet_addresses))
        .route(
            "/admin/evm-config",
            get(read_evm_config).put(write_evm_config),
        )
        .route(
            "/admin/custody/private-key",
            post(custody_private_key_disabled),
        )
        .with_state(state);
    Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("web/dist"))
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
                .allow_methods(tower_http::cors::Any),
        )
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        service: "midas",
        custody_execution: "disabled_in_v1",
    })
}
async fn openapi_yaml() -> ([(axum::http::HeaderName, &'static str); 1], &'static str) {
    (
        [(header::CONTENT_TYPE, "application/yaml; charset=utf-8")],
        include_str!("../openapi.yaml"),
    )
}

async fn auth_config() -> Json<AuthConfig> {
    Json(AuthConfig {
        auth_mini_base_url: AUTH_MINI_BASE_URL,
        audiences: vec![AUTH_MINI_AUDIENCE],
        linkit_base_url: "https://linkit.ntnl.io",
    })
}

async fn setup_status(State(state): State<AppState>) -> Result<Json<SetupStatus>, ApiError> {
    Ok(Json(SetupStatus {
        initialized: meta(&state.db, ROOT_USER_ID_KEY)
            .await
            .map_err(db_error)?
            .is_some(),
        root_user_id: meta(&state.db, ROOT_USER_ID_KEY).await.map_err(db_error)?,
    }))
}

async fn setup_initialize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<SetupRequest>,
) -> Result<Json<SetupStatus>, ApiError> {
    let caller = require_user(&state, &headers).await?;
    if caller != input.root_user_id {
        return Err(ApiError::forbidden());
    }
    if meta(&state.db, ROOT_USER_ID_KEY)
        .await
        .map_err(db_error)?
        .is_some()
    {
        return Err(ApiError::not_configured());
    }
    set_meta(&state.db, ROOT_USER_ID_KEY, &caller)
        .await
        .map_err(db_error)?;
    Ok(Json(SetupStatus {
        initialized: true,
        root_user_id: Some(caller),
    }))
}

async fn my_balance(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Balance>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let micros = sqlx::query_scalar::<_, i64>("SELECT COALESCE(SUM(balance_delta_usd_micros), 0) FROM ledger_entries WHERE user_id=?1 AND status='posted'")
        .bind(&user_id).fetch_one(&state.db).await.map_err(db_error)?;
    Ok(Json(Balance {
        currency: "USD",
        available_usd_micros: micros,
        available_usd: format_usd(micros),
    }))
}

async fn my_ledger(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LedgerEntry>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let rows = sqlx::query("SELECT e.id,e.kind,e.status,e.amount_usd_micros,e.balance_delta_usd_micros,e.created_at,e.posted_at,e.external_reference,a.symbol,a.chain_id FROM ledger_entries e LEFT JOIN supported_assets a ON a.id=e.asset_id WHERE e.user_id=?1 ORDER BY e.created_at DESC,e.id DESC LIMIT 100")
        .bind(&user_id).fetch_all(&state.db).await.map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| LedgerEntry {
                id: r.get(0),
                kind: r.get(1),
                status: r.get(2),
                amount_usd_micros: r.get(3),
                balance_delta_usd_micros: r.get(4),
                created_at: r.get(5),
                posted_at: r.get(6),
                external_reference: r.get(7),
                asset_symbol: r.get(8),
                chain_id: r.get(9),
            })
            .collect(),
    ))
}

#[derive(Serialize)]
struct WalletAddress {
    chain_id: i64,
    address: String,
    custody_status: String,
    created_at: String,
}
async fn my_wallet_addresses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WalletAddress>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let rows=sqlx::query("SELECT chain_id,address,custody_status,created_at FROM wallet_addresses WHERE user_id=?1 ORDER BY chain_id").bind(user_id).fetch_all(&state.db).await.map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| WalletAddress {
                chain_id: r.get(0),
                address: r.get(1),
                custody_status: r.get(2),
                created_at: r.get(3),
            })
            .collect(),
    ))
}

async fn read_evm_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EvmConfig>, ApiError> {
    require_root(&state, &headers).await?;
    let rows =
        sqlx::query("SELECT chain_id,name,rpc_url,enabled FROM evm_networks ORDER BY chain_id")
            .fetch_all(&state.db)
            .await
            .map_err(db_error)?;
    Ok(Json(EvmConfig {
        gas_account_address: meta(&state.db, GAS_ACCOUNT_ADDRESS_KEY)
            .await
            .map_err(db_error)?,
        collection_wallet_address: meta(&state.db, COLLECTION_WALLET_ADDRESS_KEY)
            .await
            .map_err(db_error)?,
        networks: rows
            .into_iter()
            .map(|r| EvmNetwork {
                chain_id: r.get(0),
                name: r.get(1),
                rpc_url: r.get(2),
                enabled: r.get::<i64, _>(3) == 1,
            })
            .collect(),
        custody_private_key_execution: "disabled_in_v1",
    }))
}

async fn write_evm_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<EvmConfigInput>,
) -> Result<Json<EvmConfig>, ApiError> {
    require_root(&state, &headers).await?;
    let mut tx = state.db.begin().await.map_err(db_error)?;
    if let Some(address) = input.gas_account_address.as_deref() {
        set_meta_tx(&mut tx, GAS_ACCOUNT_ADDRESS_KEY, address)
            .await
            .map_err(db_error)?;
    }
    if let Some(address) = input.collection_wallet_address.as_deref() {
        set_meta_tx(&mut tx, COLLECTION_WALLET_ADDRESS_KEY, address)
            .await
            .map_err(db_error)?;
    }
    sqlx::query("DELETE FROM evm_networks")
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    for network in input.networks {
        sqlx::query("INSERT INTO evm_networks(chain_id,name,rpc_url,enabled) VALUES(?1,?2,?3,?4)")
            .bind(network.chain_id)
            .bind(network.name)
            .bind(network.rpc_url)
            .bind(network.enabled as i64)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    read_evm_config(State(state), headers).await
}

async fn custody_private_key_disabled() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(
            serde_json::json!({"error":"custody private-key handling is intentionally disabled in v1"}),
        ),
    )
}

async fn require_user(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(ApiError::unauthorized)?;
    let principal = state
        .verifier
        .verify(token)
        .await
        .map_err(|_| ApiError::unauthorized())?;
    Uuid::parse_str(&principal.subject).map_err(|_| ApiError::unauthorized())?;
    sqlx::query("INSERT OR IGNORE INTO users(id) VALUES(?1)")
        .bind(&principal.subject)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(principal.subject)
}
async fn require_initialized_user(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<String, ApiError> {
    if meta(&state.db, ROOT_USER_ID_KEY)
        .await
        .map_err(db_error)?
        .is_none()
    {
        return Err(ApiError::not_configured());
    }
    require_user(state, headers).await
}
async fn require_root(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    let user = require_initialized_user(state, headers).await?;
    if meta(&state.db, ROOT_USER_ID_KEY)
        .await
        .map_err(db_error)?
        .as_deref()
        != Some(&user)
    {
        return Err(ApiError::forbidden());
    }
    Ok(user)
}
async fn meta(db: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT value FROM app_meta WHERE key=?1")
        .bind(key)
        .fetch_optional(db)
        .await
}
async fn set_meta(db: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app_meta(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at").bind(key).bind(value).bind(Utc::now().to_rfc3339()).execute(db).await?;
    Ok(())
}
async fn set_meta_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &str,
    value: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app_meta(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at").bind(key).bind(value).bind(Utc::now().to_rfc3339()).execute(&mut **tx).await?;
    Ok(())
}
fn db_error(_: sqlx::Error) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "database operation failed",
    )
}
fn format_usd(micros: i64) -> String {
    let sign = if micros < 0 { "-" } else { "" };
    let absolute = micros.unsigned_abs();
    format!("{sign}{}.{:06}", absolute / 1_000_000, absolute % 1_000_000)
}
async fn open_db(path: &PathBuf) -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);
    Ok(SqlitePool::connect_with(options).await?)
}
async fn migrate(db: &SqlitePool) -> anyhow::Result<()> {
    for statement in include_str!("../sql/schema.sql")
        .split(";\n")
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sqlx::query(statement).execute(db).await?;
    }
    Ok(())
}
fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("midas")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn schema_uses_wal_and_never_requires_a_private_key() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let nullable:i64=sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('wallet_key_envelopes') WHERE name='encrypted_private_key' AND \"notnull\"=0").fetch_one(&db).await.unwrap();
        assert_eq!(nullable, 1);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn formats_usd_micros() {
        assert_eq!(format_usd(1_250_000), "1.250000");
        assert_eq!(format_usd(-1), "-0.000001");
    }
}
