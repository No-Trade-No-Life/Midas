use std::{net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use auth_mini_axum::{AuthMiniVerifier, JwksCachePolicy};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::Utc;
use ethers::{
    contract::abigen,
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, H256, TransactionRequest, U256},
    utils::keccak256,
};
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
use tokio::sync::Mutex;
use tower_http::{cors::CorsLayer, services::ServeDir};
use uuid::Uuid;

abigen!(
    Erc20,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
    ]"#,
);

const AUTH_MINI_BASE_URL: &str = "https://auth.ntnl.io";
const AUTH_MINI_AUDIENCE: &str = "midas.ntnl.io";
const ROOT_USER_ID_KEY: &str = "root_user_id";
const GAS_ACCOUNT_ADDRESS_KEY: &str = "evm_gas_account_address";
const GAS_ACCOUNT_PRIVATE_KEY_KEY: &str = "evm_gas_account_private_key";
const COLLECTION_WALLET_ADDRESS_KEY: &str = "evm_collection_wallet_address";
const COLLECTION_WALLET_PRIVATE_KEY_KEY: &str = "evm_collection_wallet_private_key";
const GAS_FUNDING_WEI_KEY: &str = "evm_gas_funding_wei";
const DEFAULT_GAS_FUNDING_WEI: &str = "1000000000000000";

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    verifier: AuthMiniVerifier,
    write_lock: Arc<Mutex<()>>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "authentication is required")
    }

    fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "root access is required")
    }

    fn not_configured() -> Self {
        Self::new(StatusCode::CONFLICT, "Midas root setup is required")
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, message)
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    service: &'static str,
    custody_execution: &'static str,
}

#[derive(Serialize)]
struct Version {
    git_sha: &'static str,
    package_version: &'static str,
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
    note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct EvmNetwork {
    chain_id: i64,
    name: String,
    rpc_url: String,
    enabled: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct SupportedAsset {
    id: String,
    chain_id: i64,
    symbol: String,
    contract_address: String,
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    network_name: Option<String>,
}

#[derive(Serialize)]
struct EvmConfig {
    gas_account_address: Option<String>,
    gas_account_private_key_configured: bool,
    collection_wallet_address: Option<String>,
    collection_wallet_private_key_configured: bool,
    gas_funding_wei: String,
    networks: Vec<EvmNetwork>,
    assets: Vec<SupportedAsset>,
}

#[derive(Deserialize)]
struct EvmConfigInput {
    gas_account_private_key: Option<String>,
    collection_wallet_address: Option<String>,
    collection_wallet_private_key: Option<String>,
    gas_funding_wei: Option<String>,
    #[serde(default)]
    networks: Vec<EvmNetwork>,
    #[serde(default)]
    assets: Vec<SupportedAsset>,
}

#[derive(Serialize)]
struct WalletAddress {
    chain_id: i64,
    address: String,
    custody_status: &'static str,
    created_at: String,
}

#[derive(Deserialize)]
struct WalletProvisionRequest {
    chain_id: i64,
}

#[derive(Deserialize)]
struct DepositRequest {
    chain_id: i64,
    asset_id: String,
    transaction_hash: String,
}

#[derive(Serialize)]
struct DepositResponse {
    id: String,
    amount_usd_micros: i64,
    amount_usd: String,
    asset_symbol: String,
    transaction_hash: String,
    sweep_status: String,
}

#[derive(Deserialize)]
struct TransferRequest {
    recipient_user_id: String,
    amount_usd_micros: i64,
    note: Option<String>,
}

#[derive(Serialize)]
struct TransferResponse {
    id: String,
    amount_usd_micros: i64,
    amount_usd: String,
    recipient_user_id: String,
    status: &'static str,
}

#[derive(Deserialize)]
struct WithdrawalRequest {
    asset_id: String,
    destination_address: String,
    amount_usd_micros: i64,
    note: Option<String>,
}

#[derive(Serialize)]
struct WithdrawalResponse {
    id: String,
    asset_symbol: String,
    destination_address: String,
    amount_usd_micros: i64,
    amount_usd: String,
    transaction_hash: Option<String>,
    status: String,
}

struct DepositTarget {
    wallet_id: String,
    address: String,
    asset_id: String,
    symbol: String,
    contract_address: String,
    rpc_url: String,
}

struct VerifiedDeposit {
    amount_usd_micros: i64,
    raw_amount: String,
    transaction_hash: String,
    log_index: i64,
}

struct WithdrawalTarget {
    contract_address: String,
    rpc_url: String,
    chain_id: i64,
    destination_address: String,
    amount_usd_micros: i64,
    transaction_hash: Option<String>,
    status: String,
}

struct LedgerInsert<'a> {
    id: &'a str,
    user_id: &'a str,
    kind: &'a str,
    status: &'a str,
    asset_id: Option<&'a str>,
    amount_usd_micros: i64,
    balance_delta_usd_micros: i64,
    counterparty_user_id: Option<&'a str>,
    external_reference: Option<&'a str>,
    note: Option<&'a str>,
    now: &'a str,
}

struct OperationInsert<'a> {
    id: &'a str,
    user_id: &'a str,
    kind: &'a str,
    idempotency_key: &'a str,
    resource_id: &'a str,
    status: &'a str,
    now: &'a str,
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
    let app = app(AppState {
        db,
        verifier,
        write_lock: Arc::new(Mutex::new(())),
    });
    let addr = "127.0.0.1:8787"
        .parse::<SocketAddr>()
        .context("parse Midas listener address")?;
    println!("Midas listening on http://{addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

fn app(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .route("/openapi.yaml", get(openapi_yaml))
        .route("/auth/config", get(auth_config))
        .route("/setup/status", get(setup_status))
        .route("/setup/initialize", post(setup_initialize))
        .route("/assets", get(list_assets))
        .route("/balances/me", get(my_balance))
        .route("/ledger/me", get(my_ledger))
        .route(
            "/wallet-addresses/me",
            get(my_wallet_addresses).post(provision_wallet_address),
        )
        .route("/deposits/confirm", post(confirm_deposit))
        .route("/transfers", post(create_transfer))
        .route("/withdrawals", get(my_withdrawals).post(create_withdrawal))
        .route("/withdrawals/:id/finalize", post(finalize_withdrawal))
        .route(
            "/admin/evm-config",
            get(read_evm_config).put(write_evm_config),
        )
        .route("/admin/deposits/:id/sweep", post(retry_sweep))
        .with_state(state);
    Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new("web/dist"))
        .layer(
            CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_headers([
                    header::AUTHORIZATION,
                    header::CONTENT_TYPE,
                    header::HeaderName::from_static("idempotency-key"),
                ])
                .allow_methods(tower_http::cors::Any),
        )
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        service: "midas",
        custody_execution: "enabled_when_root_configured",
    })
}

async fn version() -> Json<Version> {
    Json(Version {
        git_sha: option_env!("MIDAS_BUILD_SHA").unwrap_or("development"),
        package_version: env!("CARGO_PKG_VERSION"),
        custody_execution: "enabled_when_root_configured",
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
    let root_user_id = meta(&state.db, ROOT_USER_ID_KEY).await.map_err(db_error)?;
    Ok(Json(SetupStatus {
        initialized: root_user_id.is_some(),
        root_user_id,
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
    let _write = state.write_lock.lock().await;
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

async fn list_assets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SupportedAsset>>, ApiError> {
    require_initialized_user(&state, &headers).await?;
    let rows = sqlx::query(
        "SELECT a.id,a.chain_id,a.symbol,a.contract_address,a.enabled,n.name FROM supported_assets a JOIN evm_networks n ON n.chain_id=a.chain_id ORDER BY a.chain_id,a.symbol",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| SupportedAsset {
                id: row.get(0),
                chain_id: row.get(1),
                symbol: row.get(2),
                contract_address: row.get(3),
                enabled: row.get::<i64, _>(4) == 1,
                network_name: Some(row.get(5)),
            })
            .collect(),
    ))
}

async fn my_balance(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Balance>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let micros = available_balance(&state.db, &user_id).await?;
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
    let rows = sqlx::query("SELECT e.id,e.kind,e.status,e.amount_usd_micros,e.balance_delta_usd_micros,e.created_at,e.posted_at,e.external_reference,e.note,a.symbol,a.chain_id FROM ledger_entries e LEFT JOIN supported_assets a ON a.id=e.asset_id WHERE e.user_id=?1 ORDER BY e.created_at DESC,e.id DESC LIMIT 100")
        .bind(&user_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(rows.into_iter().map(ledger_entry).collect()))
}

fn ledger_entry(row: sqlx::sqlite::SqliteRow) -> LedgerEntry {
    LedgerEntry {
        id: row.get(0),
        kind: row.get(1),
        status: row.get(2),
        amount_usd_micros: row.get(3),
        balance_delta_usd_micros: row.get(4),
        created_at: row.get(5),
        posted_at: row.get(6),
        external_reference: row.get(7),
        note: row.get(8),
        asset_symbol: row.get(9),
        chain_id: row.get(10),
    }
}

async fn my_wallet_addresses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WalletAddress>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let rows = sqlx::query("SELECT chain_id,address,created_at FROM wallet_addresses WHERE user_id=?1 ORDER BY chain_id")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| WalletAddress {
                chain_id: row.get(0),
                address: row.get(1),
                custody_status: "configured",
                created_at: row.get(2),
            })
            .collect(),
    ))
}

async fn provision_wallet_address(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<WalletProvisionRequest>,
) -> Result<Json<WalletAddress>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let _write = state.write_lock.lock().await;
    if let Some(row) = sqlx::query(
        "SELECT chain_id,address,created_at FROM wallet_addresses WHERE user_id=?1 AND chain_id=?2",
    )
    .bind(&user_id)
    .bind(input.chain_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_error)?
    {
        return Ok(Json(WalletAddress {
            chain_id: row.get(0),
            address: row.get(1),
            custody_status: "configured",
            created_at: row.get(2),
        }));
    }
    let enabled: Option<i64> =
        sqlx::query_scalar("SELECT enabled FROM evm_networks WHERE chain_id=?1")
            .bind(input.chain_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_error)?;
    if enabled != Some(1) {
        return Err(ApiError::invalid("the EVM network is not enabled"));
    }
    let private_key = LocalWallet::new(&mut thread_rng()).with_chain_id(input.chain_id as u64);
    let address = format!("{:#x}", private_key.address());
    let wallet_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await.map_err(db_error)?;
    sqlx::query("INSERT INTO wallet_addresses(id,user_id,chain_id,address,custody_status,created_at) VALUES(?1,?2,?3,?4,'configured',?5)")
        .bind(&wallet_id)
        .bind(&user_id)
        .bind(input.chain_id)
        .bind(&address)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("INSERT INTO wallet_private_keys(wallet_address_id,private_key,created_at) VALUES(?1,?2,?3)")
        .bind(&wallet_id)
        .bind(format!("0x{}", hex::encode(private_key.signer().to_bytes())))
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Json(WalletAddress {
        chain_id: input.chain_id,
        address,
        custody_status: "configured",
        created_at: now,
    }))
}

async fn confirm_deposit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<DepositRequest>,
) -> Result<Json<DepositResponse>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let idempotency_key = idempotency_key(&headers)?;
    let target = load_deposit_target(&state.db, &user_id, &input).await?;
    let verified = verify_deposit_receipt(&target, &input.transaction_hash).await?;

    let deposit_id = {
        let _write = state.write_lock.lock().await;
        if let Some(existing) =
            operation_resource(&state.db, &user_id, "deposit", &idempotency_key).await?
        {
            return Ok(Json(read_deposit_response(&state.db, &existing).await?));
        }
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM deposits WHERE asset_id=?1 AND transaction_hash=?2 AND log_index=?3",
        )
        .bind(&target.asset_id)
        .bind(&verified.transaction_hash)
        .bind(verified.log_index)
        .fetch_optional(&state.db)
        .await
        .map_err(db_error)?;
        if existing.is_some() {
            return Err(ApiError::conflict(
                "this Transfer event has already been credited",
            ));
        }
        let deposit_id = Uuid::new_v4().to_string();
        let ledger_id = Uuid::new_v4().to_string();
        let operation_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let reference = format!(
            "evm:{}:{}:{}",
            input.chain_id, verified.transaction_hash, verified.log_index
        );
        let mut tx = state.db.begin().await.map_err(db_error)?;
        insert_ledger(
            &mut tx,
            LedgerInsert {
                id: &ledger_id,
                user_id: &user_id,
                kind: "deposit",
                status: "posted",
                asset_id: Some(&target.asset_id),
                amount_usd_micros: verified.amount_usd_micros,
                balance_delta_usd_micros: verified.amount_usd_micros,
                counterparty_user_id: None,
                external_reference: Some(&reference),
                note: Some("Confirmed EVM deposit"),
                now: &now,
            },
        )
        .await?;
        sqlx::query("INSERT INTO deposits(id,user_id,wallet_address_id,asset_id,ledger_entry_id,transaction_hash,log_index,raw_amount,sweep_status,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'queued',?9)")
            .bind(&deposit_id)
            .bind(&user_id)
            .bind(&target.wallet_id)
            .bind(&target.asset_id)
            .bind(&ledger_id)
            .bind(&verified.transaction_hash)
            .bind(verified.log_index)
            .bind(&verified.raw_amount)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("INSERT INTO deposit_sweeps(id,deposit_id,status,created_at,updated_at) VALUES(?1,?2,'queued',?3,?3)")
            .bind(Uuid::new_v4().to_string())
            .bind(&deposit_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        insert_operation(
            &mut tx,
            OperationInsert {
                id: &operation_id,
                user_id: &user_id,
                kind: "deposit",
                idempotency_key: &idempotency_key,
                resource_id: &deposit_id,
                status: "completed",
                now: &now,
            },
        )
        .await?;
        tx.commit().await.map_err(db_error)?;
        deposit_id
    };
    let _ = submit_sweep(&state, &deposit_id).await;
    Ok(Json(read_deposit_response(&state.db, &deposit_id).await?))
}

async fn create_transfer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<TransferRequest>,
) -> Result<Json<TransferResponse>, ApiError> {
    let sender = require_initialized_user(&state, &headers).await?;
    let idempotency_key = idempotency_key(&headers)?;
    if input.amount_usd_micros <= 0 {
        return Err(ApiError::invalid(
            "transfer amount must be greater than zero",
        ));
    }
    if input.recipient_user_id == sender {
        return Err(ApiError::invalid(
            "a transfer recipient must be another Midas user",
        ));
    }
    Uuid::parse_str(&input.recipient_user_id)
        .map_err(|_| ApiError::invalid("recipient_user_id must be an Auth Mini UUID"))?;
    let _write = state.write_lock.lock().await;
    if let Some(existing) =
        operation_resource(&state.db, &sender, "transfer", &idempotency_key).await?
    {
        return Ok(Json(read_transfer_response(&state.db, &existing).await?));
    }
    let recipient_exists: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE id=?1")
        .bind(&input.recipient_user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_error)?;
    if recipient_exists.is_none() {
        return Err(ApiError::invalid(
            "the recipient has not opened a Midas account yet",
        ));
    }
    if available_balance(&state.db, &sender).await? < input.amount_usd_micros {
        return Err(ApiError::conflict(
            "the available USD balance is insufficient",
        ));
    }
    let transfer_id = Uuid::new_v4().to_string();
    let sender_ledger_id = Uuid::new_v4().to_string();
    let recipient_ledger_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await.map_err(db_error)?;
    let outgoing_reference = format!("transfer:{transfer_id}:out");
    insert_ledger(
        &mut tx,
        LedgerInsert {
            id: &sender_ledger_id,
            user_id: &sender,
            kind: "transfer_out",
            status: "posted",
            asset_id: None,
            amount_usd_micros: input.amount_usd_micros,
            balance_delta_usd_micros: -input.amount_usd_micros,
            counterparty_user_id: Some(&input.recipient_user_id),
            external_reference: Some(&outgoing_reference),
            note: input.note.as_deref(),
            now: &now,
        },
    )
    .await?;
    let incoming_reference = format!("transfer:{transfer_id}:in");
    insert_ledger(
        &mut tx,
        LedgerInsert {
            id: &recipient_ledger_id,
            user_id: &input.recipient_user_id,
            kind: "transfer_in",
            status: "posted",
            asset_id: None,
            amount_usd_micros: input.amount_usd_micros,
            balance_delta_usd_micros: input.amount_usd_micros,
            counterparty_user_id: Some(&sender),
            external_reference: Some(&incoming_reference),
            note: input.note.as_deref(),
            now: &now,
        },
    )
    .await?;
    sqlx::query("INSERT INTO internal_transfers(id,sender_user_id,recipient_user_id,amount_usd_micros,sender_ledger_entry_id,recipient_ledger_entry_id,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)")
        .bind(&transfer_id)
        .bind(&sender)
        .bind(&input.recipient_user_id)
        .bind(input.amount_usd_micros)
        .bind(&sender_ledger_id)
        .bind(&recipient_ledger_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    let operation_id = Uuid::new_v4().to_string();
    insert_operation(
        &mut tx,
        OperationInsert {
            id: &operation_id,
            user_id: &sender,
            kind: "transfer",
            idempotency_key: &idempotency_key,
            resource_id: &transfer_id,
            status: "completed",
            now: &now,
        },
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(Json(read_transfer_response(&state.db, &transfer_id).await?))
}

async fn create_withdrawal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<WithdrawalRequest>,
) -> Result<Json<WithdrawalResponse>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let idempotency_key = idempotency_key(&headers)?;
    if input.amount_usd_micros <= 0 {
        return Err(ApiError::invalid(
            "withdrawal amount must be greater than zero",
        ));
    }
    let destination = parse_address(&input.destination_address, "destination_address")?;
    let _write = state.write_lock.lock().await;
    if let Some(existing) =
        operation_resource(&state.db, &user_id, "withdrawal", &idempotency_key).await?
    {
        return Ok(Json(read_withdrawal_response(&state.db, &existing).await?));
    }
    if available_balance(&state.db, &user_id).await? < input.amount_usd_micros {
        return Err(ApiError::conflict(
            "the available USD balance is insufficient",
        ));
    }
    let asset_exists: Option<i64> =
        sqlx::query_scalar("SELECT enabled FROM supported_assets WHERE id=?1")
            .bind(&input.asset_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_error)?;
    if asset_exists != Some(1) {
        return Err(ApiError::invalid("the withdrawal asset is not enabled"));
    }
    let withdrawal_id = Uuid::new_v4().to_string();
    let ledger_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await.map_err(db_error)?;
    let withdrawal_reference = format!("withdrawal:{withdrawal_id}");
    insert_ledger(
        &mut tx,
        LedgerInsert {
            id: &ledger_id,
            user_id: &user_id,
            kind: "withdrawal",
            status: "pending",
            asset_id: Some(&input.asset_id),
            amount_usd_micros: input.amount_usd_micros,
            balance_delta_usd_micros: -input.amount_usd_micros,
            counterparty_user_id: None,
            external_reference: Some(&withdrawal_reference),
            note: input.note.as_deref(),
            now: &now,
        },
    )
    .await?;
    sqlx::query("INSERT INTO withdrawals(id,user_id,asset_id,ledger_entry_id,destination_address,amount_usd_micros,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,'awaiting_signer',?7,?7)")
        .bind(&withdrawal_id)
        .bind(&user_id)
        .bind(&input.asset_id)
        .bind(&ledger_id)
        .bind(format!("{destination:#x}"))
        .bind(input.amount_usd_micros)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    let operation_id = Uuid::new_v4().to_string();
    insert_operation(
        &mut tx,
        OperationInsert {
            id: &operation_id,
            user_id: &user_id,
            kind: "withdrawal",
            idempotency_key: &idempotency_key,
            resource_id: &withdrawal_id,
            status: "accepted",
            now: &now,
        },
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    let _ = submit_withdrawal(&state, &withdrawal_id).await;
    Ok(Json(
        read_withdrawal_response(&state.db, &withdrawal_id).await?,
    ))
}

async fn my_withdrawals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WithdrawalResponse>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM withdrawals WHERE user_id=?1 ORDER BY created_at DESC,id DESC LIMIT 100",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_error)?;
    let mut withdrawals = Vec::with_capacity(ids.len());
    for id in ids {
        withdrawals.push(read_withdrawal_response(&state.db, &id).await?);
    }
    Ok(Json(withdrawals))
}

async fn finalize_withdrawal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<WithdrawalResponse>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let target = load_withdrawal_target(&state.db, &id, Some(&user_id)).await?;
    let tx_hash = target
        .transaction_hash
        .as_deref()
        .ok_or_else(|| ApiError::conflict("the withdrawal has not been broadcast"))?;
    let provider = rpc_provider(&target.rpc_url)?;
    let receipt = provider
        .get_transaction_receipt(
            H256::from_str(tx_hash)
                .map_err(|_| ApiError::invalid("stored withdrawal transaction hash is invalid"))?,
        )
        .await
        .map_err(chain_error)?
        .ok_or_else(|| ApiError::conflict("the withdrawal transaction is not confirmed yet"))?;
    let succeeded = receipt.status.map(|status| status.as_u64()) == Some(1);
    let now = Utc::now().to_rfc3339();
    let _write = state.write_lock.lock().await;
    let mut tx = state.db.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE withdrawals SET status=?1,updated_at=?2 WHERE id=?3")
        .bind(if succeeded { "completed" } else { "failed" })
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE ledger_entries SET status=?1,posted_at=CASE WHEN ?1='posted' THEN ?2 ELSE NULL END WHERE id=(SELECT ledger_entry_id FROM withdrawals WHERE id=?3)")
        .bind(if succeeded { "posted" } else { "rejected" })
        .bind(&now)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(Json(read_withdrawal_response(&state.db, &id).await?))
}

async fn read_evm_config(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<EvmConfig>, ApiError> {
    require_root(&state, &headers).await?;
    Ok(Json(load_evm_config(&state.db).await?))
}

async fn write_evm_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<EvmConfigInput>,
) -> Result<Json<EvmConfig>, ApiError> {
    require_root(&state, &headers).await?;
    if let Some(gas_funding_wei) = input.gas_funding_wei.as_deref() {
        U256::from_dec_str(gas_funding_wei)
            .map_err(|_| ApiError::invalid("gas_funding_wei must be a decimal wei amount"))?;
    }
    for network in &input.networks {
        if network.chain_id <= 0
            || network.name.trim().is_empty()
            || !network.rpc_url.starts_with("http")
        {
            return Err(ApiError::invalid(
                "each network needs a positive chain_id, name, and HTTP(S) RPC URL",
            ));
        }
    }
    for asset in &input.assets {
        if asset.symbol != "USDC" && asset.symbol != "USDT" {
            return Err(ApiError::invalid("only USDC and USDT are supported"));
        }
        parse_address(&asset.contract_address, "asset contract_address")?;
    }
    let gas_address = input
        .gas_account_private_key
        .as_deref()
        .map(|key| parse_wallet(key, 1).map(|wallet| format!("{:#x}", wallet.address())))
        .transpose()?;
    let collection_key_address = input
        .collection_wallet_private_key
        .as_deref()
        .map(|key| parse_wallet(key, 1).map(|wallet| format!("{:#x}", wallet.address())))
        .transpose()?;
    if let (Some(configured), Some(derived)) = (
        input.collection_wallet_address.as_deref(),
        collection_key_address.as_deref(),
    ) && parse_address(configured, "collection_wallet_address")?
        != parse_address(derived, "collection_wallet_private_key")?
    {
        return Err(ApiError::invalid(
            "collection_wallet_private_key does not match collection_wallet_address",
        ));
    }
    let _write = state.write_lock.lock().await;
    let mut tx = state.db.begin().await.map_err(db_error)?;
    if let Some(key) = input.gas_account_private_key.as_deref() {
        set_meta_tx(&mut tx, GAS_ACCOUNT_PRIVATE_KEY_KEY, key).await?;
        set_meta_tx(
            &mut tx,
            GAS_ACCOUNT_ADDRESS_KEY,
            gas_address.as_deref().expect("derived gas address"),
        )
        .await?;
    }
    if let Some(address) = input.collection_wallet_address.as_deref() {
        set_meta_tx(
            &mut tx,
            COLLECTION_WALLET_ADDRESS_KEY,
            &format!(
                "{:#x}",
                parse_address(address, "collection_wallet_address")?
            ),
        )
        .await?;
    }
    if let Some(key) = input.collection_wallet_private_key.as_deref() {
        set_meta_tx(&mut tx, COLLECTION_WALLET_PRIVATE_KEY_KEY, key).await?;
        if input.collection_wallet_address.is_none() {
            set_meta_tx(
                &mut tx,
                COLLECTION_WALLET_ADDRESS_KEY,
                collection_key_address
                    .as_deref()
                    .expect("derived collection address"),
            )
            .await?;
        }
    }
    if let Some(value) = input.gas_funding_wei.as_deref() {
        set_meta_tx(&mut tx, GAS_FUNDING_WEI_KEY, value).await?;
    }
    for network in &input.networks {
        sqlx::query("INSERT INTO evm_networks(chain_id,name,rpc_url,enabled) VALUES(?1,?2,?3,?4) ON CONFLICT(chain_id) DO UPDATE SET name=excluded.name,rpc_url=excluded.rpc_url,enabled=excluded.enabled")
            .bind(network.chain_id)
            .bind(&network.name)
            .bind(&network.rpc_url)
            .bind(network.enabled as i64)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    }
    for asset in &input.assets {
        sqlx::query("INSERT INTO supported_assets(id,chain_id,symbol,contract_address,usd_scale,enabled) VALUES(?1,?2,?3,?4,6,?5) ON CONFLICT(id) DO UPDATE SET chain_id=excluded.chain_id,symbol=excluded.symbol,contract_address=excluded.contract_address,enabled=excluded.enabled")
            .bind(&asset.id)
            .bind(asset.chain_id)
            .bind(&asset.symbol)
            .bind(format!("{:#x}", parse_address(&asset.contract_address, "asset contract_address")?))
            .bind(asset.enabled as i64)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(Json(load_evm_config(&state.db).await?))
}

async fn retry_sweep(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<DepositResponse>, ApiError> {
    require_root(&state, &headers).await?;
    submit_sweep(&state, &id).await?;
    Ok(Json(read_deposit_response(&state.db, &id).await?))
}

async fn submit_sweep(state: &AppState, deposit_id: &str) -> Result<(), ApiError> {
    let row = sqlx::query("SELECT d.asset_id,d.raw_amount,w.address,k.private_key,a.contract_address,n.rpc_url,n.chain_id FROM deposits d JOIN wallet_addresses w ON w.id=d.wallet_address_id JOIN wallet_private_keys k ON k.wallet_address_id=w.id JOIN supported_assets a ON a.id=d.asset_id JOIN evm_networks n ON n.chain_id=a.chain_id WHERE d.id=?1")
        .bind(deposit_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("deposit does not exist"))?;
    let raw_amount: String = row.get(1);
    let deposit_address: String = row.get(2);
    let deposit_private_key: String = row.get(3);
    let contract_address: String = row.get(4);
    let rpc_url: String = row.get(5);
    let chain_id: i64 = row.get(6);
    let gas_private_key = meta(&state.db, GAS_ACCOUNT_PRIVATE_KEY_KEY)
        .await
        .map_err(db_error)?;
    let collection_address = meta(&state.db, COLLECTION_WALLET_ADDRESS_KEY)
        .await
        .map_err(db_error)?;
    let Some(gas_private_key) = gas_private_key else {
        mark_sweep_configuration(state, deposit_id).await?;
        return Ok(());
    };
    let Some(collection_address) = collection_address else {
        mark_sweep_configuration(state, deposit_id).await?;
        return Ok(());
    };
    let gas_funding_wei = meta(&state.db, GAS_FUNDING_WEI_KEY)
        .await
        .map_err(db_error)?
        .unwrap_or_else(|| DEFAULT_GAS_FUNDING_WEI.to_string());
    let provider = rpc_provider(&rpc_url)?;
    let gas_wallet = parse_wallet(&gas_private_key, chain_id)?;
    let gas_client = Arc::new(SignerMiddleware::new(provider.clone(), gas_wallet));
    let gas_pending = gas_client
        .send_transaction(
            TransactionRequest::pay(
                parse_address(&deposit_address, "stored deposit address")?,
                U256::from_dec_str(&gas_funding_wei)
                    .map_err(|_| ApiError::invalid("configured gas funding amount is invalid"))?,
            ),
            None,
        )
        .await
        .map_err(chain_error)?;
    let gas_transaction_hash = format!("{:#x}", gas_pending.tx_hash());
    let gas_receipt = tokio::time::timeout(Duration::from_secs(90), gas_pending)
        .await
        .map_err(|_| {
            ApiError::new(
                StatusCode::GATEWAY_TIMEOUT,
                "gas funding transaction did not confirm within 90 seconds",
            )
        })?
        .map_err(chain_error)?
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "gas funding transaction was dropped",
            )
        })?;
    if gas_receipt.status.map(|status| status.as_u64()) != Some(1) {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "gas funding transaction reverted",
        ));
    }
    let source_wallet = parse_wallet(&deposit_private_key, chain_id)?;
    let source_client = Arc::new(SignerMiddleware::new(provider, source_wallet));
    let contract = Erc20::new(
        parse_address(&contract_address, "configured token contract")?,
        source_client,
    );
    let token_call = contract.transfer(
        parse_address(&collection_address, "configured collection wallet")?,
        U256::from_dec_str(&raw_amount)
            .map_err(|_| ApiError::invalid("stored deposit amount is invalid"))?,
    );
    let token_pending = token_call.send().await.map_err(chain_error)?;
    let token_transaction_hash = format!("{:#x}", token_pending.tx_hash());
    let now = Utc::now().to_rfc3339();
    let _write = state.write_lock.lock().await;
    let mut tx = state.db.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE deposits SET sweep_status='submitted' WHERE id=?1")
        .bind(deposit_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE deposit_sweeps SET gas_transaction_hash=?1,token_transaction_hash=?2,status='submitted',error_message=NULL,updated_at=?3 WHERE deposit_id=?4")
        .bind(gas_transaction_hash)
        .bind(token_transaction_hash)
        .bind(&now)
        .bind(deposit_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(())
}

async fn mark_sweep_configuration(state: &AppState, deposit_id: &str) -> Result<(), ApiError> {
    let _write = state.write_lock.lock().await;
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE deposits SET sweep_status='awaiting_configuration' WHERE id=?1")
        .bind(deposit_id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE deposit_sweeps SET status='queued',error_message='Gas wallet or collection wallet is not configured',updated_at=?1 WHERE deposit_id=?2")
        .bind(now)
        .bind(deposit_id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn submit_withdrawal(state: &AppState, withdrawal_id: &str) -> Result<(), ApiError> {
    let target = load_withdrawal_target(&state.db, withdrawal_id, None).await?;
    if target.status != "awaiting_signer" {
        return Ok(());
    }
    let collection_private_key = meta(&state.db, COLLECTION_WALLET_PRIVATE_KEY_KEY)
        .await
        .map_err(db_error)?;
    let Some(collection_private_key) = collection_private_key else {
        return Ok(());
    };
    let collection_address = meta(&state.db, COLLECTION_WALLET_ADDRESS_KEY)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::conflict("collection wallet address is not configured"))?;
    let provider = rpc_provider(&target.rpc_url)?;
    let wallet = parse_wallet(&collection_private_key, target.chain_id)?;
    if wallet.address() != parse_address(&collection_address, "configured collection wallet")? {
        return Err(ApiError::invalid(
            "collection wallet private key does not match its configured address",
        ));
    }
    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    let contract = Erc20::new(
        parse_address(&target.contract_address, "configured token contract")?,
        client,
    );
    let transfer_call = contract.transfer(
        parse_address(&target.destination_address, "stored withdrawal destination")?,
        U256::from(target.amount_usd_micros as u64),
    );
    let pending = transfer_call.send().await.map_err(chain_error)?;
    let now = Utc::now().to_rfc3339();
    let _write = state.write_lock.lock().await;
    sqlx::query(
        "UPDATE withdrawals SET transaction_hash=?1,status='submitted',updated_at=?2 WHERE id=?3",
    )
    .bind(format!("{:#x}", pending.tx_hash()))
    .bind(now)
    .bind(withdrawal_id)
    .execute(&state.db)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn load_deposit_target(
    db: &SqlitePool,
    user_id: &str,
    input: &DepositRequest,
) -> Result<DepositTarget, ApiError> {
    let row = sqlx::query("SELECT w.id,w.address,a.id,a.symbol,a.contract_address,n.rpc_url FROM wallet_addresses w JOIN wallet_private_keys k ON k.wallet_address_id=w.id JOIN supported_assets a ON a.id=?3 AND a.chain_id=w.chain_id JOIN evm_networks n ON n.chain_id=w.chain_id WHERE w.user_id=?1 AND w.chain_id=?2 AND a.enabled=1 AND n.enabled=1")
        .bind(user_id)
        .bind(input.chain_id)
        .bind(&input.asset_id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("provision a wallet and choose an enabled asset before confirming a deposit"))?;
    Ok(DepositTarget {
        wallet_id: row.get(0),
        address: row.get(1),
        asset_id: row.get(2),
        symbol: row.get(3),
        contract_address: row.get(4),
        rpc_url: row.get(5),
    })
}

async fn verify_deposit_receipt(
    target: &DepositTarget,
    value: &str,
) -> Result<VerifiedDeposit, ApiError> {
    let transaction_hash = H256::from_str(value)
        .map_err(|_| ApiError::invalid("transaction_hash must be a 32-byte EVM hash"))?;
    let receipt = rpc_provider(&target.rpc_url)?
        .get_transaction_receipt(transaction_hash)
        .await
        .map_err(chain_error)?
        .ok_or_else(|| ApiError::conflict("the deposit transaction is not confirmed yet"))?;
    if receipt.status.map(|status| status.as_u64()) != Some(1) {
        return Err(ApiError::invalid("the deposit transaction reverted"));
    }
    let contract = parse_address(&target.contract_address, "configured token contract")?;
    let recipient = parse_address(&target.address, "stored deposit address")?;
    let transfer_topic = H256::from(keccak256("Transfer(address,address,uint256)"));
    for (log_index, log) in receipt.logs.iter().enumerate() {
        if log.address != contract || log.topics.len() != 3 || log.topics[0] != transfer_topic {
            continue;
        }
        if Address::from_slice(&log.topics[2].as_bytes()[12..]) != recipient {
            continue;
        }
        let amount = U256::from_big_endian(log.data.0.as_ref());
        if amount.is_zero() || amount > U256::from(i64::MAX as u64) {
            return Err(ApiError::invalid(
                "the confirmed token amount is outside Midas's USD ledger range",
            ));
        }
        return Ok(VerifiedDeposit {
            amount_usd_micros: amount.as_u64() as i64,
            raw_amount: amount.to_string(),
            transaction_hash: format!("{transaction_hash:#x}"),
            log_index: log_index as i64,
        });
    }
    Err(ApiError::invalid(format!(
        "the transaction does not contain a configured {} Transfer to this deposit address",
        target.symbol
    )))
}

async fn read_deposit_response(db: &SqlitePool, id: &str) -> Result<DepositResponse, ApiError> {
    let row = sqlx::query("SELECT d.id,e.amount_usd_micros,a.symbol,d.transaction_hash,d.sweep_status FROM deposits d JOIN ledger_entries e ON e.id=d.ledger_entry_id JOIN supported_assets a ON a.id=d.asset_id WHERE d.id=?1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("deposit does not exist"))?;
    let amount_usd_micros: i64 = row.get(1);
    Ok(DepositResponse {
        id: row.get(0),
        amount_usd_micros,
        amount_usd: format_usd(amount_usd_micros),
        asset_symbol: row.get(2),
        transaction_hash: row.get(3),
        sweep_status: row.get(4),
    })
}

async fn read_transfer_response(db: &SqlitePool, id: &str) -> Result<TransferResponse, ApiError> {
    let row = sqlx::query(
        "SELECT id,recipient_user_id,amount_usd_micros FROM internal_transfers WHERE id=?1",
    )
    .bind(id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?
    .ok_or_else(|| ApiError::invalid("transfer does not exist"))?;
    let amount_usd_micros: i64 = row.get(2);
    Ok(TransferResponse {
        id: row.get(0),
        recipient_user_id: row.get(1),
        amount_usd_micros,
        amount_usd: format_usd(amount_usd_micros),
        status: "posted",
    })
}

async fn read_withdrawal_response(
    db: &SqlitePool,
    id: &str,
) -> Result<WithdrawalResponse, ApiError> {
    let row = sqlx::query("SELECT w.id,a.symbol,w.destination_address,w.amount_usd_micros,w.transaction_hash,w.status FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id WHERE w.id=?1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("withdrawal does not exist"))?;
    let amount_usd_micros: i64 = row.get(3);
    Ok(WithdrawalResponse {
        id: row.get(0),
        asset_symbol: row.get(1),
        destination_address: row.get(2),
        amount_usd_micros,
        amount_usd: format_usd(amount_usd_micros),
        transaction_hash: row.get(4),
        status: row.get(5),
    })
}

async fn load_withdrawal_target(
    db: &SqlitePool,
    id: &str,
    user_id: Option<&str>,
) -> Result<WithdrawalTarget, ApiError> {
    let row = sqlx::query("SELECT a.contract_address,n.rpc_url,n.chain_id,w.destination_address,w.amount_usd_micros,w.transaction_hash,w.status FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id JOIN evm_networks n ON n.chain_id=a.chain_id WHERE w.id=?1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("withdrawal does not exist"))?;
    if let Some(user_id) = user_id {
        let owner: String = sqlx::query_scalar("SELECT user_id FROM withdrawals WHERE id=?1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(db_error)?;
        if owner != user_id {
            return Err(ApiError::forbidden());
        }
    }
    Ok(WithdrawalTarget {
        contract_address: row.get(0),
        rpc_url: row.get(1),
        chain_id: row.get(2),
        destination_address: row.get(3),
        amount_usd_micros: row.get(4),
        transaction_hash: row.get(5),
        status: row.get(6),
    })
}

async fn load_evm_config(db: &SqlitePool) -> Result<EvmConfig, ApiError> {
    let network_rows =
        sqlx::query("SELECT chain_id,name,rpc_url,enabled FROM evm_networks ORDER BY chain_id")
            .fetch_all(db)
            .await
            .map_err(db_error)?;
    let asset_rows = sqlx::query("SELECT id,chain_id,symbol,contract_address,enabled FROM supported_assets ORDER BY chain_id,symbol")
        .fetch_all(db)
        .await
        .map_err(db_error)?;
    Ok(EvmConfig {
        gas_account_address: meta(db, GAS_ACCOUNT_ADDRESS_KEY).await.map_err(db_error)?,
        gas_account_private_key_configured: meta(db, GAS_ACCOUNT_PRIVATE_KEY_KEY)
            .await
            .map_err(db_error)?
            .is_some(),
        collection_wallet_address: meta(db, COLLECTION_WALLET_ADDRESS_KEY)
            .await
            .map_err(db_error)?,
        collection_wallet_private_key_configured: meta(db, COLLECTION_WALLET_PRIVATE_KEY_KEY)
            .await
            .map_err(db_error)?
            .is_some(),
        gas_funding_wei: meta(db, GAS_FUNDING_WEI_KEY)
            .await
            .map_err(db_error)?
            .unwrap_or_else(|| DEFAULT_GAS_FUNDING_WEI.to_string()),
        networks: network_rows
            .into_iter()
            .map(|row| EvmNetwork {
                chain_id: row.get(0),
                name: row.get(1),
                rpc_url: row.get(2),
                enabled: row.get::<i64, _>(3) == 1,
            })
            .collect(),
        assets: asset_rows
            .into_iter()
            .map(|row| SupportedAsset {
                id: row.get(0),
                chain_id: row.get(1),
                symbol: row.get(2),
                contract_address: row.get(3),
                enabled: row.get::<i64, _>(4) == 1,
                network_name: None,
            })
            .collect(),
    })
}

async fn available_balance(db: &SqlitePool, user_id: &str) -> Result<i64, ApiError> {
    sqlx::query_scalar("SELECT COALESCE(SUM(balance_delta_usd_micros),0) FROM ledger_entries WHERE user_id=?1 AND status IN ('posted','pending')")
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(db_error)
}

async fn insert_ledger(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    entry: LedgerInsert<'_>,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,asset_id,amount_usd_micros,balance_delta_usd_micros,counterparty_user_id,external_reference,note,created_at,posted_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,CASE WHEN ?4='posted' THEN ?11 ELSE NULL END)")
        .bind(entry.id)
        .bind(entry.user_id)
        .bind(entry.kind)
        .bind(entry.status)
        .bind(entry.asset_id)
        .bind(entry.amount_usd_micros)
        .bind(entry.balance_delta_usd_micros)
        .bind(entry.counterparty_user_id)
        .bind(entry.external_reference)
        .bind(entry.note)
        .bind(entry.now)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn insert_operation(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    operation: OperationInsert<'_>,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO payment_operations(id,user_id,kind,idempotency_key,resource_id,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)")
        .bind(operation.id)
        .bind(operation.user_id)
        .bind(operation.kind)
        .bind(operation.idempotency_key)
        .bind(operation.resource_id)
        .bind(operation.status)
        .bind(operation.now)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn operation_resource(
    db: &SqlitePool,
    user_id: &str,
    kind: &str,
    idempotency_key: &str,
) -> Result<Option<String>, ApiError> {
    sqlx::query_scalar("SELECT resource_id FROM payment_operations WHERE user_id=?1 AND kind=?2 AND idempotency_key=?3")
        .bind(user_id)
        .bind(kind)
        .bind(idempotency_key)
        .fetch_optional(db)
        .await
        .map_err(db_error)
        .map(|value| value.flatten())
}

fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let value = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| {
            ApiError::invalid(
                "Idempotency-Key is required for payment writes and must be at most 128 characters",
            )
        })?;
    Ok(value.to_string())
}

fn parse_address(value: &str, name: &str) -> Result<Address, ApiError> {
    Address::from_str(value)
        .map_err(|_| ApiError::invalid(format!("{name} must be a valid EVM address")))
}

fn parse_wallet(value: &str, chain_id: i64) -> Result<LocalWallet, ApiError> {
    if chain_id <= 0 {
        return Err(ApiError::invalid("chain_id must be positive"));
    }
    value
        .parse::<LocalWallet>()
        .map(|wallet| wallet.with_chain_id(chain_id as u64))
        .map_err(|_| ApiError::invalid("private key must be a valid EVM signing key"))
}

fn rpc_provider(value: &str) -> Result<Provider<Http>, ApiError> {
    Provider::<Http>::try_from(value)
        .map_err(|_| ApiError::invalid("configured RPC URL is invalid"))
}

fn chain_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        format!("EVM RPC operation failed: {error}"),
    )
}

async fn require_user(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
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
    sqlx::query("INSERT INTO app_meta(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at")
        .bind(key)
        .bind(value)
        .bind(Utc::now().to_rfc3339())
        .execute(db)
        .await?;
    Ok(())
}

async fn set_meta_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    key: &str,
    value: &str,
) -> Result<(), ApiError> {
    sqlx::query("INSERT INTO app_meta(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at")
        .bind(key)
        .bind(value)
        .bind(Utc::now().to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    Ok(())
}

fn db_error(_: sqlx::Error) -> ApiError {
    ApiError::new(
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
        .filter(|statement| !statement.is_empty())
    {
        sqlx::query(statement).execute(db).await?;
    }
    Ok(())
}

fn data_dir() -> PathBuf {
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/var/lib/midas")
    }
    #[cfg(not(target_os = "linux"))]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("midas")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn schema_uses_wal_and_persists_custody_keys() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        let key_table: String = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='wallet_private_keys'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(key_table, "wallet_private_keys");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn formats_usd_micros() {
        assert_eq!(format_usd(1_250_000), "1.250000");
        assert_eq!(format_usd(-1), "-0.000001");
    }

    #[test]
    fn evm_private_keys_generate_addresses() {
        let wallet = LocalWallet::new(&mut thread_rng());
        assert!(format!("{:#x}", wallet.address()).starts_with("0x"));
    }

    #[tokio::test]
    async fn available_balance_reserves_pending_withdrawals_and_ignores_rejected_entries() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user_id)
            .execute(&db)
            .await
            .unwrap();
        for (status, delta) in [
            ("posted", 2_000_000_i64),
            ("pending", -500_000),
            ("rejected", -750_000),
        ] {
            sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,amount_usd_micros,balance_delta_usd_micros,created_at) VALUES(?1,?2,'adjustment',?3,0,?4,?5)")
                .bind(Uuid::new_v4().to_string())
                .bind(&user_id)
                .bind(status)
                .bind(delta)
                .bind(Utc::now().to_rfc3339())
                .execute(&db)
                .await
                .unwrap();
        }
        assert_eq!(available_balance(&db, &user_id).await.unwrap(), 1_500_000);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn idempotency_resource_is_scoped_to_user_and_payment_kind() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO payment_operations(id,user_id,kind,idempotency_key,resource_id,status,created_at,updated_at) VALUES(?1,?2,'transfer','same-key','transfer-id','completed',?3,?3)")
            .bind(Uuid::new_v4().to_string())
            .bind(&user_id)
            .bind(Utc::now().to_rfc3339())
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            operation_resource(&db, &user_id, "transfer", "same-key")
                .await
                .unwrap()
                .as_deref(),
            Some("transfer-id")
        );
        assert_eq!(
            operation_resource(&db, &user_id, "withdrawal", "same-key")
                .await
                .unwrap(),
            None
        );
        let _ = std::fs::remove_file(path);
    }
}
