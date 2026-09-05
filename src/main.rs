use std::{
    collections::HashSet, net::SocketAddr, path::PathBuf, str::FromStr, sync::Arc, time::Duration,
};

use anyhow::Context;
use auth_mini_axum::{AuthMiniVerifier, JwksCachePolicy};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use chrono::Utc;
use ethers::{
    contract::abigen,
    middleware::SignerMiddleware,
    providers::{Http, Middleware, Provider},
    signers::{LocalWallet, Signer},
    types::{
        Address, BlockId, BlockNumber, Bytes, H256, Log, TransactionReceipt, TransactionRequest,
        U256, transaction::eip2718::TypedTransaction,
    },
    utils::keccak256,
};
use rand::{RngCore, thread_rng};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, sqlite::SqliteConnectOptions};
use tokio::{
    sync::Mutex,
    task::JoinSet,
    time::{self, MissedTickBehavior},
};
use tower_http::{cors::CorsLayer, services::ServeDir};
use uuid::Uuid;

abigen!(
    Erc20,
    r#"[
        function transfer(address to, uint256 amount) returns (bool)
        function balanceOf(address account) view returns (uint256)
    ]"#,
);

const AUTH_MINI_BASE_URL: &str = "https://auth.ntnl.io";
const AUTH_MINI_AUDIENCE: &str = "midas.ntnl.io";
const ROOT_USER_ID_KEY: &str = "root_user_id";
const CUSTODY_WALLET_ADDRESS_KEY: &str = "evm_custody_wallet_address";
const CUSTODY_WALLET_PRIVATE_KEY_KEY: &str = "evm_custody_wallet_private_key";
const LEGACY_GAS_ACCOUNT_PRIVATE_KEY_KEY: &str = "evm_gas_account_private_key";
const LEGACY_COLLECTION_WALLET_PRIVATE_KEY_KEY: &str = "evm_collection_wallet_private_key";
const DEFAULT_GAS_FUNDING_WEI: &str = "1000000000000000";
// `wallet_addresses.chain_id` remains on disk for existing SQLite databases.
// A user deposit key is EVM-compatible, so new rows use this internal sentinel
// and no API or query exposes it as a per-chain choice.
const EVM_ADDRESS_SENTINEL_CHAIN_ID: i64 = 1;
const DEPOSIT_DISCOVERY_POLL_INTERVAL: Duration = Duration::from_secs(1);
const RPC_DISCOVERY_BOOTSTRAP_BLOCKS: i64 = 1_024;
const RPC_DISCOVERY_BLOCK_RANGE: i64 = 50;
const RPC_DISCOVERY_CONFIRMATION_BLOCKS: i64 = 2;
const ETHEREUM_DISCOVERY_RPC_URL: &str = "https://rpc.mevblocker.io";
const BSC_DISCOVERY_RPC_URL: &str = "https://bsc-rpc.publicnode.com";

#[derive(Clone, Copy)]
struct BuiltinEvmNetwork {
    chain_id: i64,
    name: &'static str,
    rpc_url: &'static str,
}

#[derive(Clone, Copy)]
struct BuiltinAsset {
    id: &'static str,
    chain_id: i64,
    symbol: &'static str,
    contract_address: &'static str,
    token_decimals: u8,
}

const BUILTIN_EVM_NETWORKS: [BuiltinEvmNetwork; 6] = [
    BuiltinEvmNetwork {
        chain_id: 1,
        name: "Ethereum",
        rpc_url: "https://ethereum-rpc.publicnode.com",
    },
    BuiltinEvmNetwork {
        chain_id: 56,
        name: "BNB Smart Chain",
        rpc_url: "https://bsc-dataseed.binance.org/",
    },
    BuiltinEvmNetwork {
        chain_id: 8453,
        name: "Base",
        rpc_url: "https://mainnet.base.org",
    },
    BuiltinEvmNetwork {
        chain_id: 42161,
        name: "Arbitrum One",
        rpc_url: "https://arb1.arbitrum.io/rpc",
    },
    BuiltinEvmNetwork {
        chain_id: 10,
        name: "OP Mainnet",
        rpc_url: "https://mainnet.optimism.io",
    },
    BuiltinEvmNetwork {
        chain_id: 137,
        name: "Polygon",
        rpc_url: "https://polygon-bor-rpc.publicnode.com",
    },
];

const BUILTIN_ASSETS: [BuiltinAsset; 12] = [
    BuiltinAsset {
        id: "1-USDC",
        chain_id: 1,
        symbol: "USDC",
        contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "1-USDT",
        chain_id: 1,
        symbol: "USDT",
        contract_address: "0xdac17f958d2ee523a2206206994597c13d831ec7",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "56-USDC",
        chain_id: 56,
        symbol: "USDC",
        contract_address: "0x8ac76a51cc950d9822d68b83fe1ad97b32cd580d",
        token_decimals: 18,
    },
    BuiltinAsset {
        id: "56-USDT",
        chain_id: 56,
        symbol: "USDT",
        contract_address: "0x55d398326f99059ff775485246999027b3197955",
        token_decimals: 18,
    },
    BuiltinAsset {
        id: "8453-USDC",
        chain_id: 8453,
        symbol: "USDC",
        contract_address: "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "8453-USDT",
        chain_id: 8453,
        symbol: "USDT",
        contract_address: "0xfde4c96c8593536e31f229ea8f37b2ada2699bb2",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "42161-USDC",
        chain_id: 42161,
        symbol: "USDC",
        contract_address: "0xaf88d065e77c8cc2239327c5edb3a432268e5831",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "42161-USDT",
        chain_id: 42161,
        symbol: "USDT",
        contract_address: "0xfd086bc7cd5c481dcc9c85ebe478a1c0b69fcbb9",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "10-USDC",
        chain_id: 10,
        symbol: "USDC",
        contract_address: "0x0b2c639c533813f4aa9d7837caf62653d097ff85",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "10-USDT",
        chain_id: 10,
        symbol: "USDT",
        contract_address: "0x01bff41798a0bcf287b996046ca68b395dbc1071",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "137-USDC",
        chain_id: 137,
        symbol: "USDC",
        contract_address: "0x3c499c542cef5e3811e1192ce70d8cc03d5c3359",
        token_decimals: 6,
    },
    BuiltinAsset {
        id: "137-USDT",
        chain_id: 137,
        symbol: "USDT",
        contract_address: "0xc2132d05d31c914a87c6611c10748aeb04b58e8f",
        token_decimals: 6,
    },
];

#[derive(Clone)]
struct AppState {
    db: SqlitePool,
    verifier: AuthMiniVerifier,
    write_lock: Arc<Mutex<()>>,
    sweep_lock: Arc<Mutex<()>>,
    withdrawal_lock: Arc<Mutex<()>>,
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
    network_name: Option<String>,
    transaction_hash: Option<String>,
    external_reference: Option<String>,
    note: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct EvmNetwork {
    chain_id: i64,
    name: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct SupportedAsset {
    id: String,
    chain_id: i64,
    symbol: String,
    contract_address: String,
    token_decimals: u8,
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    network_name: Option<String>,
}

#[derive(Serialize)]
struct EvmConfig {
    custody_wallet_address: Option<String>,
    custody_wallet_private_key_configured: bool,
    networks: Vec<EvmNetwork>,
    assets: Vec<SupportedAsset>,
}

#[derive(Deserialize)]
struct EvmConfigInput {
    custody_wallet_private_key: Option<String>,
}

#[derive(Serialize)]
struct DepositDiscoveryStatus {
    polling_interval_seconds: u64,
    last_attempt_at: Option<String>,
    last_success_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Serialize)]
struct CustodyBalancesResponse {
    custody_wallet_address: Option<String>,
    networks: Vec<CustodyNetworkBalances>,
}

#[derive(Serialize)]
struct CustodyNetworkBalances {
    chain_id: i64,
    network_name: String,
    native: CustodyAssetBalance,
    usdc: CustodyAssetBalance,
    usdt: CustodyAssetBalance,
}

#[derive(Serialize)]
struct CustodyAssetBalance {
    symbol: String,
    amount: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct WalletAddress {
    address: String,
    created_at: String,
}

#[derive(Deserialize)]
struct DepositRequest {
    asset_id: String,
    transaction_hash: String,
}

#[derive(Deserialize)]
struct DepositClaimRequest {
    chain_id: i64,
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

#[derive(Serialize)]
struct AdminDeposit {
    id: String,
    user_id: String,
    deposit_address: String,
    asset_symbol: String,
    chain_id: i64,
    network_name: String,
    amount_usd_micros: i64,
    amount_usd: String,
    transaction_hash: String,
    sweep_status: String,
    created_at: String,
    sweep_operation_status: String,
    gas_transaction_hash: Option<String>,
    token_transaction_hash: Option<String>,
    sweep_error_message: Option<String>,
    sweep_updated_at: String,
}

#[derive(Serialize)]
struct AdminWithdrawal {
    id: String,
    user_id: String,
    destination_address: String,
    asset_symbol: String,
    chain_id: i64,
    network_name: String,
    amount_usd_micros: i64,
    amount_usd: String,
    transaction_hash: Option<String>,
    status: String,
    last_error: Option<String>,
    created_at: String,
    retryable: bool,
}

#[derive(Serialize)]
struct AdminBalanceSummary {
    user_count: i64,
    funded_user_count: i64,
    total_available_usd_micros: i64,
    total_available_usd: String,
}

#[derive(Serialize)]
struct AdminUserBalance {
    user_id: String,
    created_at: String,
    available_usd_micros: i64,
    available_usd: String,
}

#[derive(Serialize)]
struct AdminBalancesResponse {
    summary: AdminBalanceSummary,
    users: Vec<AdminUserBalance>,
}

#[derive(Deserialize, Default)]
struct AdminLedgerQuery {
    kind: Option<String>,
    status: Option<String>,
    user_id: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

struct AdminLedgerFilters {
    kind: Option<String>,
    status: Option<String>,
    user_id: Option<String>,
    limit: i64,
    offset: i64,
}

#[derive(Serialize)]
struct AdminLedgerEntry {
    id: String,
    user_id: String,
    counterparty_user_id: Option<String>,
    kind: String,
    status: String,
    amount_usd_micros: i64,
    balance_delta_usd_micros: i64,
    created_at: String,
    posted_at: Option<String>,
    asset_symbol: Option<String>,
    chain_id: Option<i64>,
    network_name: Option<String>,
    transaction_hash: Option<String>,
    external_reference: Option<String>,
    note: Option<String>,
}

#[derive(Serialize)]
struct AdminLedgerPage {
    entries: Vec<AdminLedgerEntry>,
    total: i64,
    limit: i64,
    offset: i64,
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
    address_book_entry_id: Option<String>,
    destination_label: Option<String>,
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
    token_decimals: u8,
}

struct DiscoveryTarget {
    wallet_id: String,
    user_id: String,
    address: String,
    chain_id: i64,
    next_block_number: i64,
    last_seen_block_number: i64,
}

struct DiscoveryCursorPosition {
    next_block_number: i64,
    last_seen_block_number: i64,
}

struct DiscoveredTransfer {
    hash: String,
    contract_address: String,
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
    token_decimals: u8,
    transaction_hash: Option<String>,
    signed_transaction: Option<String>,
    status: String,
}

#[derive(Serialize)]
struct AddressBookEntry {
    id: String,
    chain_id: i64,
    chain_name: String,
    label: String,
    address: String,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
struct AddressBookEntryInput {
    chain_id: i64,
    label: String,
    address: String,
}

#[derive(Deserialize)]
struct AddressBookEntryUpdate {
    label: String,
}

#[derive(Deserialize)]
struct WithdrawalAddressBookInput {
    label: String,
}

#[derive(Serialize)]
struct WithdrawalAddressTarget {
    note_id: Option<String>,
    asset_id: String,
    asset_symbol: String,
    chain_id: i64,
    network_name: String,
    address: String,
    label: Option<String>,
    last_withdrawn_at: String,
}

#[derive(Deserialize)]
struct WithdrawalTargetNoteInput {
    asset_id: String,
    address: String,
    label: String,
}

#[derive(Serialize)]
struct PaymentAgreement {
    id: String,
    owner_user_id: String,
    name: String,
    api_key_prefix: String,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
struct PaymentAgreementCreated {
    agreement: PaymentAgreement,
    api_key: String,
}

#[derive(Serialize)]
struct PaymentAgreementDetail {
    agreement: PaymentAgreement,
    bound: bool,
}

#[derive(Deserialize)]
struct PaymentAgreementInput {
    name: String,
}

#[derive(Serialize)]
struct PaymentAgreementBinding {
    agreement: PaymentAgreement,
    created_at: String,
}

#[derive(Deserialize)]
struct PaymentAgreementChargeInput {
    user_id: String,
    amount_usd_micros: i64,
}

#[derive(Serialize, Debug)]
struct PaymentAgreementCharge {
    id: String,
    agreement_id: String,
    payer_user_id: String,
    owner_user_id: String,
    amount_usd_micros: i64,
    amount_usd: String,
    created_at: String,
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
    let state = AppState {
        db,
        verifier,
        write_lock: Arc::new(Mutex::new(())),
        sweep_lock: Arc::new(Mutex::new(())),
        withdrawal_lock: Arc::new(Mutex::new(())),
    };
    resume_submitted_withdrawals(state.clone());
    start_deposit_discovery(state.clone());
    let app = app(state);
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
        .route("/wallet-addresses/me", get(my_wallet_addresses))
        .route("/deposits/confirm", post(confirm_deposit))
        .route("/deposits/claim", post(claim_deposit))
        .route("/transfers", post(create_transfer))
        .route(
            "/address-book/me",
            get(my_address_book).post(create_address_book_entry),
        )
        .route(
            "/address-book/me/:id",
            put(update_address_book_entry).delete(delete_address_book_entry),
        )
        .route(
            "/withdrawal-targets/me",
            get(my_withdrawal_targets).put(save_withdrawal_target_note),
        )
        .route(
            "/withdrawal-targets/me/:id",
            axum::routing::delete(delete_withdrawal_target_note),
        )
        .route("/withdrawals", get(my_withdrawals).post(create_withdrawal))
        .route(
            "/withdrawals/:id/address-book",
            post(save_withdrawal_destination),
        )
        .route("/withdrawals/:id/finalize", post(finalize_withdrawal))
        .route(
            "/agreements/owned",
            get(my_owned_agreements).post(create_payment_agreement),
        )
        .route(
            "/agreements/bindings/me",
            get(my_payment_agreement_bindings),
        )
        .route("/agreements/:id", get(payment_agreement_detail))
        .route(
            "/agreements/:id/bind",
            post(bind_payment_agreement).delete(unbind_payment_agreement),
        )
        .route("/agreements/:id/charges", post(charge_payment_agreement))
        .route(
            "/admin/evm-config",
            get(read_evm_config).put(write_evm_config),
        )
        .route(
            "/admin/deposit-discovery",
            get(read_deposit_discovery_status),
        )
        .route("/admin/custody-balances", get(read_custody_balances))
        .route("/admin/balances", get(list_admin_balances))
        .route("/admin/ledger", get(list_admin_ledger))
        .route("/admin/deposits", get(list_admin_deposits))
        .route("/admin/deposits/:id/sweep", post(retry_sweep))
        .route("/admin/withdrawals", get(list_admin_withdrawals))
        .route("/admin/withdrawals/:id/retry", post(retry_withdrawal))
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
                    header::HeaderName::from_static("x-api-key"),
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
        audiences: vec![AUTH_MINI_AUDIENCE, "linkit.ntnl.io"],
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
    Ok(Json(builtin_assets()))
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
    Ok(Json(load_user_ledger(&state.db, &user_id).await?))
}

async fn load_user_ledger(db: &SqlitePool, user_id: &str) -> Result<Vec<LedgerEntry>, ApiError> {
    let rows = sqlx::query("SELECT e.id,e.kind,e.status,e.amount_usd_micros,e.balance_delta_usd_micros,e.created_at,e.posted_at,e.external_reference,e.note,a.symbol,a.chain_id,n.name,COALESCE(d.transaction_hash,w.transaction_hash) FROM ledger_entries e LEFT JOIN supported_assets a ON a.id=e.asset_id LEFT JOIN evm_networks n ON n.chain_id=a.chain_id LEFT JOIN deposits d ON d.ledger_entry_id=e.id LEFT JOIN withdrawals w ON w.ledger_entry_id=e.id WHERE e.user_id=?1 ORDER BY e.created_at DESC,e.id DESC LIMIT 100")
        .bind(user_id)
        .fetch_all(db)
        .await
        .map_err(db_error)?;
    Ok(rows.into_iter().map(ledger_entry).collect())
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
        network_name: row.get(11),
        transaction_hash: row.get(12),
    }
}

async fn my_wallet_addresses(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WalletAddress>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let row = sqlx::query("SELECT address,created_at FROM wallet_addresses WHERE user_id=?1 ORDER BY created_at,id LIMIT 1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "deposit wallet was not initialized"))?;
    Ok(Json(vec![wallet_address(row)]))
}

async fn ensure_user_wallet(state: &AppState, user_id: &str) -> Result<WalletAddress, ApiError> {
    let _write = state.write_lock.lock().await;
    provision_user_wallet(&state.db, user_id).await
}

async fn provision_user_wallet(db: &SqlitePool, user_id: &str) -> Result<WalletAddress, ApiError> {
    let mut tx = db.begin().await.map_err(db_error)?;
    let wallet = provision_user_wallet_in_transaction(&mut tx, user_id).await?;
    tx.commit().await.map_err(db_error)?;
    Ok(wallet)
}

async fn provision_user_wallet_in_transaction(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    user_id: &str,
) -> Result<WalletAddress, ApiError> {
    if let Some(row) = sqlx::query(
        "SELECT address,created_at FROM wallet_addresses WHERE user_id=?1 ORDER BY created_at,id LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_error)?
    {
        return Ok(wallet_address(row));
    }
    let private_key = LocalWallet::new(&mut thread_rng());
    let address = format!("{:#x}", private_key.address());
    let wallet_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO wallet_addresses(id,user_id,chain_id,address,custody_status,created_at) VALUES(?1,?2,?3,?4,'configured',?5)")
        .bind(&wallet_id)
        .bind(user_id)
        .bind(EVM_ADDRESS_SENTINEL_CHAIN_ID)
        .bind(&address)
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    sqlx::query("INSERT INTO wallet_private_keys(wallet_address_id,private_key,created_at) VALUES(?1,?2,?3)")
        .bind(&wallet_id)
        .bind(format!("0x{}", hex::encode(private_key.signer().to_bytes())))
        .bind(&now)
        .execute(&mut **tx)
        .await
        .map_err(db_error)?;
    Ok(WalletAddress {
        address,
        created_at: now,
    })
}

fn wallet_address(row: sqlx::sqlite::SqliteRow) -> WalletAddress {
    WalletAddress {
        address: row.get(0),
        created_at: row.get(1),
    }
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
        insert_verified_deposit(
            &state.db,
            &user_id,
            &target,
            &verified,
            &idempotency_key,
            "Confirmed EVM deposit",
        )
        .await?
        .ok_or_else(|| ApiError::conflict("this Transfer event has already been credited"))?
    };
    start_deposit_sweep(&state, &deposit_id).await?;
    Ok(Json(read_deposit_response(&state.db, &deposit_id).await?))
}

async fn claim_deposit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<DepositClaimRequest>,
) -> Result<Json<DepositResponse>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let idempotency_key = idempotency_key(&headers)?;
    let targets = load_deposit_targets_for_chain(&state.db, &user_id, input.chain_id).await?;
    let (target, verified) = verify_deposit_claim(targets, &input.transaction_hash).await?;

    let deposit_id = {
        let _write = state.write_lock.lock().await;
        if let Some(existing) =
            operation_resource(&state.db, &user_id, "deposit", &idempotency_key).await?
        {
            return Ok(Json(read_deposit_response(&state.db, &existing).await?));
        }
        insert_verified_deposit(
            &state.db,
            &user_id,
            &target,
            &verified,
            &idempotency_key,
            "Customer-claimed EVM deposit",
        )
        .await?
        .ok_or_else(|| ApiError::conflict("this transaction has already been credited"))?
    };
    start_deposit_sweep(&state, &deposit_id).await?;
    Ok(Json(read_deposit_response(&state.db, &deposit_id).await?))
}

async fn insert_verified_deposit(
    db: &SqlitePool,
    user_id: &str,
    target: &DepositTarget,
    verified: &VerifiedDeposit,
    idempotency_key: &str,
    note: &str,
) -> Result<Option<String>, ApiError> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM deposits WHERE transaction_hash=?1")
            .bind(&verified.transaction_hash)
            .fetch_optional(db)
            .await
            .map_err(db_error)?;
    if existing.is_some() {
        return Ok(None);
    }
    let deposit_id = Uuid::new_v4().to_string();
    let ledger_id = Uuid::new_v4().to_string();
    let operation_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let reference = format!(
        "evm:{}:{}:{}",
        target.asset_id, verified.transaction_hash, verified.log_index
    );
    let mut tx = db.begin().await.map_err(db_error)?;
    insert_ledger(
        &mut tx,
        LedgerInsert {
            id: &ledger_id,
            user_id,
            kind: "deposit",
            status: "posted",
            asset_id: Some(&target.asset_id),
            amount_usd_micros: verified.amount_usd_micros,
            balance_delta_usd_micros: verified.amount_usd_micros,
            counterparty_user_id: None,
            external_reference: Some(&reference),
            note: Some(note),
            now: &now,
        },
    )
    .await?;
    sqlx::query("INSERT INTO deposits(id,user_id,wallet_address_id,asset_id,ledger_entry_id,transaction_hash,log_index,raw_amount,sweep_status,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'queued',?9)")
        .bind(&deposit_id)
        .bind(user_id)
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
            user_id,
            kind: "deposit",
            idempotency_key,
            resource_id: &deposit_id,
            status: "completed",
            now: &now,
        },
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(Some(deposit_id))
}

async fn start_deposit_sweep(state: &AppState, deposit_id: &str) -> Result<(), ApiError> {
    if let Err(error) = submit_sweep(state, deposit_id).await {
        mark_sweep_failed(state, deposit_id, &error.message).await?;
    }
    Ok(())
}

fn spawn_deposit_sweep(state: AppState, deposit_id: String) {
    tokio::spawn(async move {
        if let Err(error) = start_deposit_sweep(&state, &deposit_id).await {
            eprintln!(
                "failed to start automatic collection for deposit {deposit_id}: {}",
                error.message
            );
        }
    });
}

fn start_deposit_discovery(state: AppState) {
    tokio::spawn(async move {
        let mut interval = time::interval(DEPOSIT_DISCOVERY_POLL_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(error) = discover_next_deposit(&state).await {
                eprintln!("RPC deposit discovery cycle failed: {}", error.message);
            }
        }
    });
}

async fn discover_next_deposit(state: &AppState) -> Result<(), ApiError> {
    let Some(target) = next_discovery_target(&state.db).await? else {
        return Ok(());
    };
    let outcome: Result<DiscoveryCursorPosition, ApiError> = async {
        let (transfers, position) = rpc_discovery_transfers(&target).await?;
        process_discovered_transfers(state, &target, &transfers).await?;
        Ok(position)
    }
    .await;
    match outcome {
        Ok(position) => record_discovery_success(state, &target, &position).await,
        Err(error) => {
            record_discovery_failure(state, &target, &error.message).await?;
            Err(error)
        }
    }
}

async fn next_discovery_target(db: &SqlitePool) -> Result<Option<DiscoveryTarget>, ApiError> {
    let row = sqlx::query("SELECT w.id,w.user_id,w.address,n.chain_id,COALESCE(c.next_block_number,0),COALESCE(c.last_seen_block_number,0) FROM wallet_addresses w JOIN wallet_private_keys k ON k.wallet_address_id=w.id CROSS JOIN evm_networks n LEFT JOIN deposit_discovery_cursors c ON c.wallet_address_id=w.id AND c.chain_id=n.chain_id WHERE n.enabled=1 ORDER BY CASE WHEN c.last_attempt_at IS NULL THEN 0 ELSE 1 END,c.last_attempt_at,w.created_at,w.id,n.chain_id LIMIT 1")
        .fetch_optional(db)
        .await
        .map_err(db_error)?;
    Ok(row.map(|row| DiscoveryTarget {
        wallet_id: row.get(0),
        user_id: row.get(1),
        address: row.get(2),
        chain_id: row.get(3),
        next_block_number: row.get(4),
        last_seen_block_number: row.get(5),
    }))
}

async fn rpc_discovery_transfers(
    target: &DiscoveryTarget,
) -> Result<(Vec<DiscoveredTransfer>, DiscoveryCursorPosition), ApiError> {
    let network =
        builtin_network(target.chain_id).expect("discovery target uses a built-in EVM network");
    let provider = rpc_provider(discovery_rpc_url(network))?;
    let head_block = provider
        .get_block_number()
        .await
        .map_err(chain_error)?
        .as_u64() as i64;
    let latest_block = head_block.saturating_sub(RPC_DISCOVERY_CONFIRMATION_BLOCKS);
    let Some((start_block, end_block)) =
        rpc_discovery_range(target.next_block_number, latest_block)
    else {
        return Ok((
            Vec::new(),
            DiscoveryCursorPosition {
                next_block_number: target.next_block_number,
                last_seen_block_number: target.last_seen_block_number,
            },
        ));
    };
    let contracts: Vec<&str> = BUILTIN_ASSETS
        .iter()
        .filter(|asset| asset.chain_id == target.chain_id)
        .map(|asset| asset.contract_address)
        .collect();
    let recipient = parse_address(&target.address, "stored deposit address")?;
    let mut recipient_topic = [0_u8; 32];
    recipient_topic[12..].copy_from_slice(recipient.as_bytes());
    let filter = serde_json::json!({
        "fromBlock": format!("0x{start_block:x}"),
        "toBlock": format!("0x{end_block:x}"),
        "address": contracts,
        "topics": [
            format!("{:#x}", H256::from(keccak256("Transfer(address,address,uint256)"))),
            serde_json::Value::Null,
            format!("{:#x}", H256::from(recipient_topic)),
        ],
    });
    let logs: Vec<Log> = provider
        .request("eth_getLogs", [filter])
        .await
        .map_err(chain_error)?;
    let transfers = logs
        .into_iter()
        .filter_map(|log| {
            Some(DiscoveredTransfer {
                hash: format!("{:#x}", log.transaction_hash?),
                contract_address: format!("{:#x}", log.address),
            })
        })
        .collect();
    Ok((
        transfers,
        DiscoveryCursorPosition {
            next_block_number: end_block.saturating_add(1),
            last_seen_block_number: end_block,
        },
    ))
}

fn rpc_discovery_range(next_block_number: i64, latest_block: i64) -> Option<(i64, i64)> {
    let start_block = if next_block_number == 0 {
        latest_block.saturating_sub(RPC_DISCOVERY_BOOTSTRAP_BLOCKS - 1)
    } else {
        next_block_number
    };
    if start_block > latest_block {
        return None;
    }
    Some((
        start_block,
        latest_block.min(start_block.saturating_add(RPC_DISCOVERY_BLOCK_RANGE - 1)),
    ))
}

fn discovery_rpc_url(network: BuiltinEvmNetwork) -> &'static str {
    match network.chain_id {
        1 => ETHEREUM_DISCOVERY_RPC_URL,
        56 => BSC_DISCOVERY_RPC_URL,
        _ => network.rpc_url,
    }
}

async fn process_discovered_transfers(
    state: &AppState,
    target: &DiscoveryTarget,
    transfers: &[DiscoveredTransfer],
) -> Result<(), ApiError> {
    let mut seen = HashSet::new();
    for transfer in transfers {
        let Some(asset) = BUILTIN_ASSETS.iter().copied().find(|asset| {
            asset.chain_id == target.chain_id
                && asset
                    .contract_address
                    .eq_ignore_ascii_case(&transfer.contract_address)
        }) else {
            continue;
        };
        if !seen.insert(transfer.hash.to_ascii_lowercase()) {
            continue;
        }
        let deposit_target =
            deposit_target(target.wallet_id.clone(), target.address.clone(), asset);
        let verified = verify_deposit_receipt(&deposit_target, &transfer.hash).await?;
        let idempotency_key = format!(
            "rpc:{}:{}:{}",
            deposit_target.asset_id, verified.transaction_hash, verified.log_index
        );
        let deposit_id = {
            let _write = state.write_lock.lock().await;
            insert_verified_deposit(
                &state.db,
                &target.user_id,
                &deposit_target,
                &verified,
                &idempotency_key,
                "Automatically discovered EVM deposit",
            )
            .await?
        };
        if let Some(deposit_id) = deposit_id {
            spawn_deposit_sweep(state.clone(), deposit_id);
        }
    }
    Ok(())
}

async fn record_discovery_success(
    state: &AppState,
    target: &DiscoveryTarget,
    position: &DiscoveryCursorPosition,
) -> Result<(), ApiError> {
    let _write = state.write_lock.lock().await;
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO deposit_discovery_cursors(wallet_address_id,chain_id,next_block_number,last_seen_block_number,last_attempt_at,last_success_at,last_error,updated_at) VALUES(?1,?2,?3,?4,?5,?5,NULL,?5) ON CONFLICT(wallet_address_id,chain_id) DO UPDATE SET next_block_number=excluded.next_block_number,last_seen_block_number=excluded.last_seen_block_number,last_attempt_at=excluded.last_attempt_at,last_success_at=excluded.last_success_at,last_error=NULL,updated_at=excluded.updated_at")
        .bind(&target.wallet_id)
        .bind(target.chain_id)
        .bind(position.next_block_number)
        .bind(position.last_seen_block_number)
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(())
}

async fn record_discovery_failure(
    state: &AppState,
    target: &DiscoveryTarget,
    error_message: &str,
) -> Result<(), ApiError> {
    let _write = state.write_lock.lock().await;
    let now = Utc::now().to_rfc3339();
    let error_message: String = error_message.chars().take(500).collect();
    sqlx::query("INSERT INTO deposit_discovery_cursors(wallet_address_id,chain_id,next_block_number,last_seen_block_number,last_attempt_at,last_success_at,last_error,updated_at) VALUES(?1,?2,?3,?4,?5,NULL,?6,?5) ON CONFLICT(wallet_address_id,chain_id) DO UPDATE SET last_attempt_at=excluded.last_attempt_at,last_error=excluded.last_error,updated_at=excluded.updated_at")
        .bind(&target.wallet_id)
        .bind(target.chain_id)
        .bind(target.next_block_number)
        .bind(target.last_seen_block_number)
        .bind(&now)
        .bind(error_message)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(())
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
            "a transfer recipient must be another user",
        ));
    }
    Uuid::parse_str(&input.recipient_user_id)
        .map_err(|_| ApiError::invalid("recipient_user_id must be an Auth Mini UUID"))?;
    let _write = state.write_lock.lock().await;
    Ok(Json(
        post_transfer(&state.db, &sender, &input, &idempotency_key).await?,
    ))
}

async fn post_transfer(
    db: &SqlitePool,
    sender: &str,
    input: &TransferRequest,
    idempotency_key: &str,
) -> Result<TransferResponse, ApiError> {
    if let Some(existing) = operation_resource(db, sender, "transfer", idempotency_key).await? {
        return read_transfer_response(db, &existing).await;
    }
    if available_balance(db, sender).await? < input.amount_usd_micros {
        return Err(ApiError::conflict(
            "the available USD balance is insufficient",
        ));
    }
    let transfer_id = Uuid::new_v4().to_string();
    let sender_ledger_id = Uuid::new_v4().to_string();
    let recipient_ledger_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let mut tx = db.begin().await.map_err(db_error)?;
    sqlx::query("INSERT OR IGNORE INTO users(id) VALUES(?1)")
        .bind(&input.recipient_user_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    provision_user_wallet_in_transaction(&mut tx, &input.recipient_user_id).await?;
    let outgoing_reference = format!("transfer:{transfer_id}:out");
    insert_ledger(
        &mut tx,
        LedgerInsert {
            id: &sender_ledger_id,
            user_id: sender,
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
            counterparty_user_id: Some(sender),
            external_reference: Some(&incoming_reference),
            note: input.note.as_deref(),
            now: &now,
        },
    )
    .await?;
    sqlx::query("INSERT INTO internal_transfers(id,sender_user_id,recipient_user_id,amount_usd_micros,sender_ledger_entry_id,recipient_ledger_entry_id,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)")
        .bind(&transfer_id)
        .bind(sender)
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
            user_id: sender,
            kind: "transfer",
            idempotency_key,
            resource_id: &transfer_id,
            status: "completed",
            now: &now,
        },
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    read_transfer_response(db, &transfer_id).await
}

async fn my_withdrawal_targets(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WithdrawalAddressTarget>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    Ok(Json(load_withdrawal_targets(&state.db, &user_id).await?))
}

async fn load_withdrawal_targets(
    db: &SqlitePool,
    user_id: &str,
) -> Result<Vec<WithdrawalAddressTarget>, ApiError> {
    let rows = sqlx::query("SELECT n.id,a.id,a.symbol,a.chain_id,e.name,w.destination_address,n.label,MAX(w.created_at) FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id JOIN evm_networks e ON e.chain_id=a.chain_id LEFT JOIN withdrawal_target_notes n ON n.user_id=w.user_id AND n.asset_id=w.asset_id AND n.address=w.destination_address WHERE w.user_id=?1 AND w.transaction_hash IS NOT NULL GROUP BY n.id,a.id,a.symbol,a.chain_id,e.name,w.destination_address,n.label ORDER BY MAX(w.created_at) DESC,w.destination_address,a.id")
        .bind(user_id)
        .fetch_all(db)
        .await
        .map_err(db_error)?;
    Ok(rows.into_iter().map(withdrawal_address_target).collect())
}

async fn save_withdrawal_target_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<WithdrawalTargetNoteInput>,
) -> Result<Json<WithdrawalAddressTarget>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    builtin_asset(&input.asset_id)
        .ok_or_else(|| ApiError::invalid("the withdrawal asset is not supported"))?;
    let address = format!("{:#x}", parse_address(&input.address, "address")?);
    let label = address_book_label(&input.label)?;
    let used: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM withdrawals WHERE user_id=?1 AND asset_id=?2 AND destination_address=?3 AND transaction_hash IS NOT NULL)")
        .bind(&user_id)
        .bind(&input.asset_id)
        .bind(&address)
        .fetch_one(&state.db)
        .await
        .map_err(db_error)?;
    if !used {
        return Err(ApiError::invalid(
            "only a previously broadcast withdrawal destination can be saved",
        ));
    }
    let now = Utc::now().to_rfc3339();
    let _write = state.write_lock.lock().await;
    sqlx::query("INSERT INTO withdrawal_target_notes(id,user_id,asset_id,address,label,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6) ON CONFLICT(user_id,asset_id,address) DO UPDATE SET label=excluded.label,updated_at=excluded.updated_at")
        .bind(Uuid::new_v4().to_string())
        .bind(&user_id)
        .bind(&input.asset_id)
        .bind(&address)
        .bind(&label)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(
        read_withdrawal_target(&state.db, &user_id, &input.asset_id, &address).await?,
    ))
}

async fn read_withdrawal_target(
    db: &SqlitePool,
    user_id: &str,
    asset_id: &str,
    address: &str,
) -> Result<WithdrawalAddressTarget, ApiError> {
    let row = sqlx::query("SELECT n.id,a.id,a.symbol,a.chain_id,e.name,w.destination_address,n.label,MAX(w.created_at) FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id JOIN evm_networks e ON e.chain_id=a.chain_id LEFT JOIN withdrawal_target_notes n ON n.user_id=w.user_id AND n.asset_id=w.asset_id AND n.address=w.destination_address WHERE w.user_id=?1 AND a.id=?2 AND w.destination_address=?3 AND w.transaction_hash IS NOT NULL GROUP BY n.id,a.id,a.symbol,a.chain_id,e.name,w.destination_address,n.label")
        .bind(user_id)
        .bind(asset_id)
        .bind(address)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("withdrawal target does not exist"))?;
    Ok(withdrawal_address_target(row))
}

fn withdrawal_address_target(row: sqlx::sqlite::SqliteRow) -> WithdrawalAddressTarget {
    WithdrawalAddressTarget {
        note_id: row.get(0),
        asset_id: row.get(1),
        asset_symbol: row.get(2),
        chain_id: row.get(3),
        network_name: row.get(4),
        address: row.get(5),
        label: row.get(6),
        last_withdrawn_at: row.get(7),
    }
}

async fn delete_withdrawal_target_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let _write = state.write_lock.lock().await;
    let deleted = sqlx::query("DELETE FROM withdrawal_target_notes WHERE id=?1 AND user_id=?2")
        .bind(id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::invalid("withdrawal target note does not exist"));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn my_owned_agreements(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PaymentAgreement>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let rows = sqlx::query("SELECT id,owner_user_id,name,api_key_prefix,created_at,updated_at FROM payment_agreements WHERE owner_user_id=?1 ORDER BY created_at DESC,id DESC")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(rows.into_iter().map(payment_agreement).collect()))
}

async fn create_payment_agreement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<PaymentAgreementInput>,
) -> Result<Json<PaymentAgreementCreated>, ApiError> {
    let owner_user_id = require_initialized_user(&state, &headers).await?;
    let name = payment_agreement_name(&input.name)?;
    let api_key = new_payment_agreement_api_key();
    let api_key_hash = payment_agreement_api_key_hash(&api_key);
    let api_key_prefix: String = api_key.chars().take(24).collect();
    let agreement = PaymentAgreement {
        id: Uuid::new_v4().to_string(),
        owner_user_id,
        name,
        api_key_prefix,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    let _write = state.write_lock.lock().await;
    sqlx::query("INSERT INTO payment_agreements(id,owner_user_id,name,api_key_hash,api_key_prefix,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)")
        .bind(&agreement.id)
        .bind(&agreement.owner_user_id)
        .bind(&agreement.name)
        .bind(api_key_hash)
        .bind(&agreement.api_key_prefix)
        .bind(&agreement.created_at)
        .bind(&agreement.updated_at)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(PaymentAgreementCreated { agreement, api_key }))
}

async fn payment_agreement_detail(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<PaymentAgreementDetail>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let agreement = read_payment_agreement(&state.db, &id).await?;
    let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payment_agreement_bindings WHERE agreement_id=?1 AND user_id=?2)")
        .bind(&agreement.id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(PaymentAgreementDetail { agreement, bound }))
}

async fn bind_payment_agreement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<PaymentAgreementBinding>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let agreement = read_payment_agreement(&state.db, &id).await?;
    if agreement.owner_user_id == user_id {
        return Err(ApiError::invalid(
            "the agreement owner cannot bind their own payment channel",
        ));
    }
    let now = Utc::now().to_rfc3339();
    let _write = state.write_lock.lock().await;
    sqlx::query("INSERT OR IGNORE INTO payment_agreement_bindings(agreement_id,user_id,created_at) VALUES(?1,?2,?3)")
        .bind(&agreement.id)
        .bind(&user_id)
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    let created_at: String = sqlx::query_scalar(
        "SELECT created_at FROM payment_agreement_bindings WHERE agreement_id=?1 AND user_id=?2",
    )
    .bind(&agreement.id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_error)?;
    Ok(Json(PaymentAgreementBinding {
        agreement,
        created_at,
    }))
}

async fn unbind_payment_agreement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let _write = state.write_lock.lock().await;
    sqlx::query("DELETE FROM payment_agreement_bindings WHERE agreement_id=?1 AND user_id=?2")
        .bind(id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn my_payment_agreement_bindings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PaymentAgreementBinding>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let rows = sqlx::query("SELECT a.id,a.owner_user_id,a.name,a.api_key_prefix,a.created_at,a.updated_at,b.created_at FROM payment_agreement_bindings b JOIN payment_agreements a ON a.id=b.agreement_id WHERE b.user_id=?1 ORDER BY b.created_at DESC,a.id DESC")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| PaymentAgreementBinding {
                agreement: payment_agreement_from_columns(&row, 0),
                created_at: row.get(6),
            })
            .collect(),
    ))
}

async fn charge_payment_agreement(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<PaymentAgreementChargeInput>,
) -> Result<Json<PaymentAgreementCharge>, ApiError> {
    let api_key = payment_agreement_api_key(&headers)?;
    let idempotency_key = idempotency_key(&headers)?;
    if input.amount_usd_micros <= 0 {
        return Err(ApiError::invalid(
            "automatic charge amount must be greater than zero",
        ));
    }
    Uuid::parse_str(&input.user_id)
        .map_err(|_| ApiError::invalid("user_id must be an Auth Mini UUID"))?;
    let agreement = read_payment_agreement_for_api_key(&state.db, &id, api_key).await?;
    let _write = state.write_lock.lock().await;
    Ok(Json(
        post_payment_agreement_charge(
            &state.db,
            &agreement,
            &input.user_id,
            input.amount_usd_micros,
            &idempotency_key,
        )
        .await?,
    ))
}

async fn post_payment_agreement_charge(
    db: &SqlitePool,
    agreement: &PaymentAgreement,
    payer_user_id: &str,
    amount_usd_micros: i64,
    idempotency_key: &str,
) -> Result<PaymentAgreementCharge, ApiError> {
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM payment_agreement_charges WHERE agreement_id=?1 AND idempotency_key=?2",
    )
    .bind(&agreement.id)
    .bind(idempotency_key)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;
    if let Some(existing) = existing {
        return read_payment_agreement_charge(db, &existing).await;
    }
    let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM payment_agreement_bindings WHERE agreement_id=?1 AND user_id=?2)")
        .bind(&agreement.id)
        .bind(payer_user_id)
        .fetch_one(db)
        .await
        .map_err(db_error)?;
    if !bound {
        return Err(ApiError::conflict(
            "this user has not authorized the automatic payment channel",
        ));
    }
    if available_balance(db, payer_user_id).await? < amount_usd_micros {
        return Err(ApiError::conflict(
            "the user's available USD balance is insufficient",
        ));
    }
    let charge_id = Uuid::new_v4().to_string();
    let payer_ledger_id = Uuid::new_v4().to_string();
    let owner_ledger_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let note = format!("Automatic charge: {}", agreement.name);
    let payer_reference = format!("agreement:{charge_id}:out");
    let owner_reference = format!("agreement:{charge_id}:in");
    let mut tx = db.begin().await.map_err(db_error)?;
    insert_ledger(
        &mut tx,
        LedgerInsert {
            id: &payer_ledger_id,
            user_id: payer_user_id,
            kind: "transfer_out",
            status: "posted",
            asset_id: None,
            amount_usd_micros,
            balance_delta_usd_micros: -amount_usd_micros,
            counterparty_user_id: Some(&agreement.owner_user_id),
            external_reference: Some(&payer_reference),
            note: Some(&note),
            now: &now,
        },
    )
    .await?;
    insert_ledger(
        &mut tx,
        LedgerInsert {
            id: &owner_ledger_id,
            user_id: &agreement.owner_user_id,
            kind: "transfer_in",
            status: "posted",
            asset_id: None,
            amount_usd_micros,
            balance_delta_usd_micros: amount_usd_micros,
            counterparty_user_id: Some(payer_user_id),
            external_reference: Some(&owner_reference),
            note: Some(&note),
            now: &now,
        },
    )
    .await?;
    sqlx::query("INSERT INTO payment_agreement_charges(id,agreement_id,payer_user_id,amount_usd_micros,payer_ledger_entry_id,owner_ledger_entry_id,idempotency_key,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
        .bind(&charge_id)
        .bind(&agreement.id)
        .bind(payer_user_id)
        .bind(amount_usd_micros)
        .bind(&payer_ledger_id)
        .bind(&owner_ledger_id)
        .bind(idempotency_key)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    read_payment_agreement_charge(db, &charge_id).await
}

fn payment_agreement_name(value: &str) -> Result<String, ApiError> {
    let name = value.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "payment channel name must contain 1 to 80 visible characters",
        ));
    }
    Ok(name.to_string())
}

fn new_payment_agreement_api_key() -> String {
    let mut bytes = [0_u8; 32];
    thread_rng().fill_bytes(&mut bytes);
    format!("midas_agreement_{}", hex::encode(bytes))
}

fn payment_agreement_api_key_hash(api_key: &str) -> String {
    hex::encode(keccak256(api_key.as_bytes()))
}

fn payment_agreement_api_key(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "a valid agreement API key is required",
            )
        })
}

async fn read_payment_agreement(db: &SqlitePool, id: &str) -> Result<PaymentAgreement, ApiError> {
    let row = sqlx::query("SELECT id,owner_user_id,name,api_key_prefix,created_at,updated_at FROM payment_agreements WHERE id=?1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("payment channel does not exist"))?;
    Ok(payment_agreement(row))
}

async fn read_payment_agreement_for_api_key(
    db: &SqlitePool,
    id: &str,
    api_key: &str,
) -> Result<PaymentAgreement, ApiError> {
    let row = sqlx::query("SELECT id,owner_user_id,name,api_key_prefix,created_at,updated_at FROM payment_agreements WHERE id=?1 AND api_key_hash=?2")
        .bind(id)
        .bind(payment_agreement_api_key_hash(api_key))
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "a valid agreement API key is required"))?;
    Ok(payment_agreement(row))
}

fn payment_agreement(row: sqlx::sqlite::SqliteRow) -> PaymentAgreement {
    payment_agreement_from_columns(&row, 0)
}

fn payment_agreement_from_columns(
    row: &sqlx::sqlite::SqliteRow,
    offset: usize,
) -> PaymentAgreement {
    PaymentAgreement {
        id: row.get(offset),
        owner_user_id: row.get(offset + 1),
        name: row.get(offset + 2),
        api_key_prefix: row.get(offset + 3),
        created_at: row.get(offset + 4),
        updated_at: row.get(offset + 5),
    }
}

async fn read_payment_agreement_charge(
    db: &SqlitePool,
    id: &str,
) -> Result<PaymentAgreementCharge, ApiError> {
    let row = sqlx::query("SELECT c.id,c.agreement_id,c.payer_user_id,a.owner_user_id,c.amount_usd_micros,c.created_at FROM payment_agreement_charges c JOIN payment_agreements a ON a.id=c.agreement_id WHERE c.id=?1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "automatic charge was not recorded"))?;
    let amount_usd_micros: i64 = row.get(4);
    Ok(PaymentAgreementCharge {
        id: row.get(0),
        agreement_id: row.get(1),
        payer_user_id: row.get(2),
        owner_user_id: row.get(3),
        amount_usd_micros,
        amount_usd: format_usd(amount_usd_micros),
        created_at: row.get(5),
    })
}

async fn my_address_book(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AddressBookEntry>>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let rows = sqlx::query("SELECT id,chain_id,label,address,created_at,updated_at FROM address_book_entries WHERE user_id=?1 ORDER BY chain_id,label COLLATE NOCASE,id")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?;
    rows.into_iter()
        .map(address_book_entry)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

async fn create_address_book_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<AddressBookEntryInput>,
) -> Result<Json<AddressBookEntry>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let network = builtin_network(input.chain_id)
        .ok_or_else(|| ApiError::invalid("the address book network is not supported"))?;
    let label = address_book_label(&input.label)?;
    let address = format!("{:#x}", parse_address(&input.address, "address")?);
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let _write = state.write_lock.lock().await;
    let created = sqlx::query("INSERT OR IGNORE INTO address_book_entries(id,user_id,chain_id,label,address,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)")
        .bind(&id)
        .bind(&user_id)
        .bind(network.chain_id)
        .bind(&label)
        .bind(&address)
        .bind(&now)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    if created.rows_affected() == 0 {
        return Err(ApiError::conflict(
            "this address is already in the address book for the selected network",
        ));
    }
    Ok(Json(AddressBookEntry {
        id,
        chain_id: network.chain_id,
        chain_name: network.name.to_string(),
        label,
        address,
        created_at: now.clone(),
        updated_at: now,
    }))
}

async fn update_address_book_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<AddressBookEntryUpdate>,
) -> Result<Json<AddressBookEntry>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let label = address_book_label(&input.label)?;
    let now = Utc::now().to_rfc3339();
    let _write = state.write_lock.lock().await;
    let updated = sqlx::query(
        "UPDATE address_book_entries SET label=?1,updated_at=?2 WHERE id=?3 AND user_id=?4",
    )
    .bind(&label)
    .bind(&now)
    .bind(&id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(db_error)?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::invalid("address book entry does not exist"));
    }
    read_address_book_entry(&state.db, &user_id, &id)
        .await
        .map(Json)
}

async fn delete_address_book_entry(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let _write = state.write_lock.lock().await;
    let used: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM withdrawals WHERE user_id=?1 AND address_book_entry_id=?2",
    )
    .bind(&user_id)
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(db_error)?;
    if used > 0 {
        return Err(ApiError::conflict(
            "an address used by a withdrawal cannot be removed",
        ));
    }
    let deleted = sqlx::query("DELETE FROM address_book_entries WHERE id=?1 AND user_id=?2")
        .bind(&id)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::invalid("address book entry does not exist"));
    }
    Ok(StatusCode::NO_CONTENT)
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
    builtin_asset(&input.asset_id)
        .ok_or_else(|| ApiError::invalid("the withdrawal asset is not supported"))?;
    let destination_address = format!(
        "{:#x}",
        parse_address(&input.destination_address, "destination_address")?
    );
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
    sqlx::query("INSERT INTO withdrawals(id,user_id,asset_id,ledger_entry_id,address_book_entry_id,destination_address,amount_usd_micros,status,created_at,updated_at) VALUES(?1,?2,?3,?4,NULL,?5,?6,'awaiting_signer',?7,?7)")
        .bind(&withdrawal_id)
        .bind(&user_id)
        .bind(&input.asset_id)
        .bind(&ledger_id)
        .bind(&destination_address)
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
    let response = read_withdrawal_response(&state.db, &withdrawal_id).await?;
    spawn_withdrawal_submission(state.clone(), withdrawal_id);
    Ok(Json(response))
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

async fn save_withdrawal_destination(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<WithdrawalAddressBookInput>,
) -> Result<Json<AddressBookEntry>, ApiError> {
    let user_id = require_initialized_user(&state, &headers).await?;
    let label = address_book_label(&input.label)?;
    let _write = state.write_lock.lock().await;
    Ok(Json(
        save_withdrawal_destination_for_user(&state.db, &user_id, &id, &label).await?,
    ))
}

async fn save_withdrawal_destination_for_user(
    db: &SqlitePool,
    user_id: &str,
    id: &str,
    label: &str,
) -> Result<AddressBookEntry, ApiError> {
    let withdrawal = sqlx::query("SELECT a.chain_id,w.destination_address,w.address_book_entry_id FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id WHERE w.id=?1 AND w.user_id=?2")
        .bind(id)
        .bind(user_id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("withdrawal does not exist"))?;
    let chain_id: i64 = withdrawal.get(0);
    let destination_address: String = withdrawal.get(1);
    let linked_entry_id: Option<String> = withdrawal.get(2);
    if let Some(entry_id) = linked_entry_id {
        return read_address_book_entry(db, user_id, &entry_id).await;
    }
    let entry_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM address_book_entries WHERE user_id=?1 AND chain_id=?2 AND address=?3",
    )
    .bind(user_id)
    .bind(chain_id)
    .bind(&destination_address)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;
    let entry_id = match entry_id {
        Some(entry_id) => entry_id,
        None => {
            let entry_id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO address_book_entries(id,user_id,chain_id,label,address,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)")
                .bind(&entry_id)
                .bind(user_id)
                .bind(chain_id)
                .bind(label)
                .bind(&destination_address)
                .bind(&now)
                .execute(db)
                .await
                .map_err(db_error)?;
            entry_id
        }
    };
    sqlx::query(
        "UPDATE withdrawals SET address_book_entry_id=?1,updated_at=?2 WHERE id=?3 AND user_id=?4",
    )
    .bind(&entry_id)
    .bind(Utc::now().to_rfc3339())
    .bind(id)
    .bind(user_id)
    .execute(db)
    .await
    .map_err(db_error)?;
    read_address_book_entry(db, user_id, &entry_id).await
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
    settle_withdrawal(
        &state.db,
        &state.write_lock,
        &id,
        receipt.status.map(|status| status.as_u64()) == Some(1),
    )
    .await?;
    Ok(Json(read_withdrawal_response(&state.db, &id).await?))
}

async fn settle_withdrawal(
    db: &SqlitePool,
    write_lock: &Mutex<()>,
    withdrawal_id: &str,
    succeeded: bool,
) -> Result<(), ApiError> {
    let now = Utc::now().to_rfc3339();
    let _write = write_lock.lock().await;
    let mut tx = db.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE withdrawals SET status=?1,updated_at=?2 WHERE id=?3")
        .bind(if succeeded { "completed" } else { "failed" })
        .bind(&now)
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE withdrawals SET last_error=?1 WHERE id=?2")
        .bind(if succeeded {
            None
        } else {
            Some("the withdrawal transaction reverted on-chain")
        })
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE ledger_entries SET status=?1,posted_at=CASE WHEN ?1='posted' THEN ?2 ELSE NULL END WHERE id=(SELECT ledger_entry_id FROM withdrawals WHERE id=?3)")
        .bind(if succeeded { "posted" } else { "rejected" })
        .bind(&now)
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE payment_operations SET status=?1,updated_at=?2 WHERE kind='withdrawal' AND resource_id=?3")
        .bind(if succeeded { "completed" } else { "failed" })
        .bind(&now)
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(())
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
    let custody_address = input
        .custody_wallet_private_key
        .as_deref()
        .map(|key| parse_wallet(key, 1).map(|wallet| format!("{:#x}", wallet.address())))
        .transpose()?;
    let _write = state.write_lock.lock().await;
    let mut tx = state.db.begin().await.map_err(db_error)?;
    if let Some(key) = input.custody_wallet_private_key.as_deref() {
        set_meta_tx(&mut tx, CUSTODY_WALLET_PRIVATE_KEY_KEY, key).await?;
        set_meta_tx(
            &mut tx,
            CUSTODY_WALLET_ADDRESS_KEY,
            custody_address.as_deref().expect("derived custody address"),
        )
        .await?;
    }
    tx.commit().await.map_err(db_error)?;
    Ok(Json(load_evm_config(&state.db).await?))
}

async fn read_deposit_discovery_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DepositDiscoveryStatus>, ApiError> {
    require_root(&state, &headers).await?;
    Ok(Json(load_deposit_discovery_status(&state.db).await?))
}

async fn load_deposit_discovery_status(
    db: &SqlitePool,
) -> Result<DepositDiscoveryStatus, ApiError> {
    let row = sqlx::query("SELECT last_attempt_at,last_success_at,last_error FROM deposit_discovery_cursors WHERE last_attempt_at IS NOT NULL ORDER BY last_attempt_at DESC,wallet_address_id,chain_id LIMIT 1")
        .fetch_optional(db)
        .await
        .map_err(db_error)?;
    Ok(DepositDiscoveryStatus {
        polling_interval_seconds: DEPOSIT_DISCOVERY_POLL_INTERVAL.as_secs(),
        last_attempt_at: row.as_ref().map(|row| row.get(0)).unwrap_or(None),
        last_success_at: row.as_ref().map(|row| row.get(1)).unwrap_or(None),
        last_error: row.as_ref().map(|row| row.get(2)).unwrap_or(None),
    })
}

async fn read_custody_balances(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CustodyBalancesResponse>, ApiError> {
    require_root(&state, &headers).await?;
    Ok(Json(load_custody_balances(&state.db).await?))
}

async fn list_admin_deposits(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminDeposit>>, ApiError> {
    require_root(&state, &headers).await?;
    let rows = sqlx::query("SELECT d.id,d.user_id,w.address,a.symbol,n.chain_id,n.name,e.amount_usd_micros,d.transaction_hash,d.sweep_status,d.created_at,s.status,s.gas_transaction_hash,s.token_transaction_hash,s.error_message,s.updated_at FROM deposits d JOIN wallet_addresses w ON w.id=d.wallet_address_id JOIN supported_assets a ON a.id=d.asset_id JOIN evm_networks n ON n.chain_id=a.chain_id JOIN ledger_entries e ON e.id=d.ledger_entry_id JOIN deposit_sweeps s ON s.deposit_id=d.id ORDER BY d.created_at DESC,d.id DESC LIMIT 100")
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                let amount_usd_micros: i64 = row.get(6);
                AdminDeposit {
                    id: row.get(0),
                    user_id: row.get(1),
                    deposit_address: row.get(2),
                    asset_symbol: row.get(3),
                    chain_id: row.get(4),
                    network_name: row.get(5),
                    amount_usd_micros,
                    amount_usd: format_usd(amount_usd_micros),
                    transaction_hash: row.get(7),
                    sweep_status: row.get(8),
                    created_at: row.get(9),
                    sweep_operation_status: row.get(10),
                    gas_transaction_hash: row.get(11),
                    token_transaction_hash: row.get(12),
                    sweep_error_message: row.get(13),
                    sweep_updated_at: row.get(14),
                }
            })
            .collect(),
    ))
}

async fn list_admin_withdrawals(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminWithdrawal>>, ApiError> {
    require_root(&state, &headers).await?;
    let rows = sqlx::query("SELECT w.id,w.user_id,w.destination_address,a.symbol,n.chain_id,n.name,w.amount_usd_micros,w.transaction_hash,w.signed_transaction,w.status,w.last_error,w.created_at FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id JOIN evm_networks n ON n.chain_id=a.chain_id ORDER BY w.created_at DESC,w.id DESC LIMIT 100")
        .fetch_all(&state.db)
        .await
        .map_err(db_error)?;
    Ok(Json(
        rows.into_iter()
            .map(|row| {
                let amount_usd_micros: i64 = row.get(6);
                let transaction_hash: Option<String> = row.get(7);
                let signed_transaction: Option<String> = row.get(8);
                let status: String = row.get(9);
                AdminWithdrawal {
                    id: row.get(0),
                    user_id: row.get(1),
                    destination_address: row.get(2),
                    asset_symbol: row.get(3),
                    chain_id: row.get(4),
                    network_name: row.get(5),
                    amount_usd_micros,
                    amount_usd: format_usd(amount_usd_micros),
                    transaction_hash: transaction_hash.clone(),
                    status: status.clone(),
                    last_error: row.get(10),
                    created_at: row.get(11),
                    retryable: withdrawal_is_retryable(
                        &status,
                        transaction_hash.as_deref(),
                        signed_transaction.as_deref(),
                    ),
                }
            })
            .collect(),
    ))
}

async fn retry_withdrawal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<WithdrawalResponse>, ApiError> {
    require_root(&state, &headers).await?;
    let target = load_withdrawal_target(&state.db, &id, None).await?;
    if !withdrawal_is_retryable(
        &target.status,
        target.transaction_hash.as_deref(),
        target.signed_transaction.as_deref(),
    ) {
        return Err(ApiError::conflict(
            "this withdrawal cannot be retried without creating a second payment",
        ));
    }
    let response = read_withdrawal_response(&state.db, &id).await?;
    spawn_withdrawal_submission(state, id);
    Ok(Json(response))
}

async fn list_admin_balances(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AdminBalancesResponse>, ApiError> {
    require_root(&state, &headers).await?;
    Ok(Json(load_admin_balances(&state.db).await?))
}

async fn load_admin_balances(db: &SqlitePool) -> Result<AdminBalancesResponse, ApiError> {
    let summary = sqlx::query("SELECT COUNT(*),COALESCE(SUM(CASE WHEN available_usd_micros>0 THEN 1 ELSE 0 END),0),COALESCE(SUM(available_usd_micros),0) FROM (SELECT u.id,COALESCE(SUM(CASE WHEN e.status IN ('posted','pending') THEN e.balance_delta_usd_micros ELSE 0 END),0) AS available_usd_micros FROM users u LEFT JOIN ledger_entries e ON e.user_id=u.id GROUP BY u.id)")
        .fetch_one(db)
        .await
        .map_err(db_error)?;
    let total_available_usd_micros: i64 = summary.get(2);
    let users = sqlx::query("SELECT u.id,u.created_at,COALESCE(SUM(CASE WHEN e.status IN ('posted','pending') THEN e.balance_delta_usd_micros ELSE 0 END),0) FROM users u LEFT JOIN ledger_entries e ON e.user_id=u.id GROUP BY u.id,u.created_at ORDER BY 3 DESC,u.created_at DESC,u.id DESC")
        .fetch_all(db)
        .await
        .map_err(db_error)?
        .into_iter()
        .map(|row| {
            let available_usd_micros: i64 = row.get(2);
            AdminUserBalance {
                user_id: row.get(0),
                created_at: row.get(1),
                available_usd_micros,
                available_usd: format_usd(available_usd_micros),
            }
        })
        .collect();
    Ok(AdminBalancesResponse {
        summary: AdminBalanceSummary {
            user_count: summary.get(0),
            funded_user_count: summary.get(1),
            total_available_usd_micros,
            total_available_usd: format_usd(total_available_usd_micros),
        },
        users,
    })
}

async fn list_admin_ledger(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AdminLedgerQuery>,
) -> Result<Json<AdminLedgerPage>, ApiError> {
    require_root(&state, &headers).await?;
    Ok(Json(
        load_admin_ledger(&state.db, admin_ledger_filters(query)?).await?,
    ))
}

fn admin_ledger_filters(query: AdminLedgerQuery) -> Result<AdminLedgerFilters, ApiError> {
    let kind = admin_ledger_filter(
        query.kind,
        &[
            "deposit",
            "withdrawal",
            "transfer_in",
            "transfer_out",
            "adjustment",
        ],
        "kind",
    )?;
    let status = admin_ledger_filter(
        query.status,
        &["pending", "posted", "rejected", "disabled"],
        "status",
    )?;
    let user_id = query
        .user_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(user_id) = user_id.as_deref() {
        Uuid::parse_str(user_id)
            .map_err(|_| ApiError::invalid("user_id must be an Auth Mini UUID"))?;
    }
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    let offset = query.offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::invalid("offset must not be negative"));
    }
    Ok(AdminLedgerFilters {
        kind,
        status,
        user_id,
        limit,
        offset,
    })
}

fn admin_ledger_filter(
    value: Option<String>,
    allowed: &[&str],
    name: &str,
) -> Result<Option<String>, ApiError> {
    let value = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(value) = value.as_deref()
        && !allowed.contains(&value)
    {
        return Err(ApiError::invalid(format!("{name} is not supported")));
    }
    Ok(value)
}

async fn load_admin_ledger(
    db: &SqlitePool,
    filters: AdminLedgerFilters,
) -> Result<AdminLedgerPage, ApiError> {
    let kind = filters.kind.as_deref();
    let status = filters.status.as_deref();
    let user_id = filters.user_id.as_deref();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries e WHERE (?1 IS NULL OR e.kind=?1) AND (?2 IS NULL OR e.status=?2) AND (?3 IS NULL OR e.user_id=?3)")
        .bind(kind)
        .bind(status)
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(db_error)?;
    let rows = sqlx::query("SELECT e.id,e.user_id,e.counterparty_user_id,e.kind,e.status,e.amount_usd_micros,e.balance_delta_usd_micros,e.created_at,e.posted_at,e.external_reference,e.note,a.symbol,a.chain_id,n.name,COALESCE(d.transaction_hash,w.transaction_hash) FROM ledger_entries e LEFT JOIN supported_assets a ON a.id=e.asset_id LEFT JOIN evm_networks n ON n.chain_id=a.chain_id LEFT JOIN deposits d ON d.ledger_entry_id=e.id LEFT JOIN withdrawals w ON w.ledger_entry_id=e.id WHERE (?1 IS NULL OR e.kind=?1) AND (?2 IS NULL OR e.status=?2) AND (?3 IS NULL OR e.user_id=?3) ORDER BY e.created_at DESC,e.id DESC LIMIT ?4 OFFSET ?5")
        .bind(kind)
        .bind(status)
        .bind(user_id)
        .bind(filters.limit)
        .bind(filters.offset)
        .fetch_all(db)
        .await
        .map_err(db_error)?;
    Ok(AdminLedgerPage {
        entries: rows
            .into_iter()
            .map(|row| AdminLedgerEntry {
                id: row.get(0),
                user_id: row.get(1),
                counterparty_user_id: row.get(2),
                kind: row.get(3),
                status: row.get(4),
                amount_usd_micros: row.get(5),
                balance_delta_usd_micros: row.get(6),
                created_at: row.get(7),
                posted_at: row.get(8),
                external_reference: row.get(9),
                note: row.get(10),
                asset_symbol: row.get(11),
                chain_id: row.get(12),
                network_name: row.get(13),
                transaction_hash: row.get(14),
            })
            .collect(),
        total,
        limit: filters.limit,
        offset: filters.offset,
    })
}

async fn retry_sweep(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<DepositResponse>, ApiError> {
    require_root(&state, &headers).await?;
    let _sweep = state.sweep_lock.lock().await;
    require_retryable_sweep(&state.db, &id).await?;
    if let Err(error) = submit_sweep_locked(&state, &id).await {
        mark_sweep_failed(&state, &id, &error.message).await?;
        return Err(error);
    }
    Ok(Json(read_deposit_response(&state.db, &id).await?))
}

async fn submit_sweep(state: &AppState, deposit_id: &str) -> Result<(), ApiError> {
    let _sweep = state.sweep_lock.lock().await;
    let status: Option<String> =
        sqlx::query_scalar("SELECT sweep_status FROM deposits WHERE id=?1")
            .bind(deposit_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_error)?;
    if matches!(status.as_deref(), Some("submitted") | Some("swept")) {
        return Ok(());
    }
    submit_sweep_locked(state, deposit_id).await
}

async fn submit_sweep_locked(state: &AppState, deposit_id: &str) -> Result<(), ApiError> {
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
    let custody_private_key = meta(&state.db, CUSTODY_WALLET_PRIVATE_KEY_KEY)
        .await
        .map_err(db_error)?;
    let custody_address = meta(&state.db, CUSTODY_WALLET_ADDRESS_KEY)
        .await
        .map_err(db_error)?;
    let Some(custody_private_key) = custody_private_key else {
        mark_sweep_configuration(state, deposit_id).await?;
        return Ok(());
    };
    let Some(custody_address) = custody_address else {
        mark_sweep_configuration(state, deposit_id).await?;
        return Ok(());
    };
    let provider = rpc_provider(&rpc_url)?;
    let custody_wallet = parse_wallet(&custody_private_key, chain_id)?;
    if custody_wallet.address() != parse_address(&custody_address, "configured custody wallet")? {
        return Err(ApiError::invalid(
            "custody wallet private key does not match its configured address",
        ));
    }
    let gas_client = Arc::new(SignerMiddleware::new(provider.clone(), custody_wallet));
    let gas_pending = gas_client
        .send_transaction(
            TransactionRequest::pay(
                parse_address(&deposit_address, "stored deposit address")?,
                U256::from_dec_str(DEFAULT_GAS_FUNDING_WEI)
                    .expect("default gas funding amount is valid"),
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
        parse_address(&custody_address, "configured custody wallet")?,
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

async fn require_retryable_sweep(db: &SqlitePool, deposit_id: &str) -> Result<(), ApiError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT sweep_status FROM deposits WHERE id=?1")
            .bind(deposit_id)
            .fetch_optional(db)
            .await
            .map_err(db_error)?;
    match status.as_deref() {
        Some(value) if sweep_is_retryable(value) => Ok(()),
        Some(_) => Err(ApiError::conflict(
            "the collection has already been submitted and cannot be retried",
        )),
        None => Err(ApiError::invalid("deposit does not exist")),
    }
}

fn sweep_is_retryable(status: &str) -> bool {
    matches!(status, "queued" | "awaiting_configuration" | "failed")
}

async fn mark_sweep_failed(
    state: &AppState,
    deposit_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    let _write = state.write_lock.lock().await;
    let now = Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE deposits SET sweep_status='failed' WHERE id=?1")
        .bind(deposit_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE deposit_sweeps SET status='failed',error_message=?1,updated_at=?2 WHERE deposit_id=?3")
        .bind(error_message)
        .bind(&now)
        .bind(deposit_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

async fn mark_sweep_configuration(state: &AppState, deposit_id: &str) -> Result<(), ApiError> {
    let _write = state.write_lock.lock().await;
    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE deposits SET sweep_status='awaiting_configuration' WHERE id=?1")
        .bind(deposit_id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE deposit_sweeps SET status='queued',error_message='Custody wallet is not configured',updated_at=?1 WHERE deposit_id=?2")
        .bind(now)
        .bind(deposit_id)
        .execute(&state.db)
        .await
        .map_err(db_error)?;
    Ok(())
}

fn spawn_withdrawal_submission(state: AppState, withdrawal_id: String) {
    tokio::spawn(async move {
        if let Err(error) = submit_withdrawal(&state, &withdrawal_id).await
            && let Err(record_error) =
                record_withdrawal_error(&state, &withdrawal_id, &error.message).await
        {
            eprintln!(
                "failed to record withdrawal {withdrawal_id} error after {}: {}",
                error.message, record_error.message
            );
        }
    });
}

fn resume_submitted_withdrawals(state: AppState) {
    tokio::spawn(async move {
        let ids: Result<Vec<String>, _> = sqlx::query_scalar(
            "SELECT id FROM withdrawals WHERE status='submitted' AND transaction_hash IS NOT NULL",
        )
        .fetch_all(&state.db)
        .await;
        let Ok(ids) = ids else {
            eprintln!("failed to load submitted withdrawals for recovery");
            return;
        };
        for id in ids {
            spawn_withdrawal_submission(state.clone(), id);
        }
    });
}

async fn submit_withdrawal(state: &AppState, withdrawal_id: &str) -> Result<(), ApiError> {
    let (provider, transaction_hash) = {
        let _withdrawal = state.withdrawal_lock.lock().await;
        let target = load_withdrawal_target(&state.db, withdrawal_id, None).await?;
        if target.status == "submitted" && settle_submitted_withdrawal(state, withdrawal_id).await?
        {
            return Ok(());
        }
        if !withdrawal_is_retryable(
            &target.status,
            target.transaction_hash.as_deref(),
            target.signed_transaction.as_deref(),
        ) {
            return Ok(());
        }
        let (provider, signed_transaction, transaction_hash) = match target
            .signed_transaction
            .as_deref()
        {
            Some(raw) => (
                rpc_provider(&target.rpc_url)?,
                signed_raw_transaction(raw)?,
                H256::from_str(
                    target
                        .transaction_hash
                        .as_deref()
                        .expect("retryable submitted withdrawal has a transaction hash"),
                )
                .map_err(|_| ApiError::invalid("stored withdrawal transaction hash is invalid"))?,
            ),
            None => {
                let (provider, raw, transaction_hash) = sign_withdrawal(&state.db, &target).await?;
                let _write = state.write_lock.lock().await;
                persist_signed_withdrawal(&state.db, withdrawal_id, transaction_hash, &raw).await?;
                (provider, raw, transaction_hash)
            }
        };
        match provider.send_raw_transaction(signed_transaction).await {
            Ok(_) => {}
            Err(error) if error.to_string().contains("already known") => {}
            Err(error) => return Err(chain_error(error)),
        }
        (provider, transaction_hash)
    };
    let receipt = wait_for_transaction_receipt(&provider, transaction_hash).await?;
    settle_withdrawal(
        &state.db,
        &state.write_lock,
        withdrawal_id,
        receipt.status.map(|status| status.as_u64()) == Some(1),
    )
    .await
}

async fn settle_submitted_withdrawal(
    state: &AppState,
    withdrawal_id: &str,
) -> Result<bool, ApiError> {
    let target = load_withdrawal_target(&state.db, withdrawal_id, None).await?;
    let Some(receipt) = withdrawal_transaction_receipt(&target).await? else {
        return Ok(false);
    };
    settle_withdrawal(
        &state.db,
        &state.write_lock,
        withdrawal_id,
        receipt.status.map(|status| status.as_u64()) == Some(1),
    )
    .await?;
    Ok(true)
}

async fn withdrawal_transaction_receipt(
    target: &WithdrawalTarget,
) -> Result<Option<TransactionReceipt>, ApiError> {
    let tx_hash = target
        .transaction_hash
        .as_deref()
        .ok_or_else(|| ApiError::conflict("the withdrawal has not been broadcast"))?;
    rpc_provider(&target.rpc_url)?
        .get_transaction_receipt(
            H256::from_str(tx_hash)
                .map_err(|_| ApiError::invalid("stored withdrawal transaction hash is invalid"))?,
        )
        .await
        .map_err(chain_error)
}

async fn wait_for_transaction_receipt(
    provider: &Provider<Http>,
    transaction_hash: H256,
) -> Result<TransactionReceipt, ApiError> {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if let Some(receipt) = provider
                .get_transaction_receipt(transaction_hash)
                .await
                .map_err(chain_error)?
            {
                return Ok(receipt);
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
    .await
    .map_err(|_| ApiError::conflict("the withdrawal transaction confirmation timed out"))?
}

async fn sign_withdrawal(
    db: &SqlitePool,
    target: &WithdrawalTarget,
) -> Result<(Provider<Http>, Bytes, H256), ApiError> {
    let custody_private_key = meta(db, CUSTODY_WALLET_PRIVATE_KEY_KEY)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::conflict("custody wallet private key is not configured"))?;
    let custody_address = meta(db, CUSTODY_WALLET_ADDRESS_KEY)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::conflict("custody wallet address is not configured"))?;
    let wallet = parse_wallet(&custody_private_key, target.chain_id)?;
    if wallet.address() != parse_address(&custody_address, "configured custody wallet")? {
        return Err(ApiError::invalid(
            "custody wallet private key does not match its configured address",
        ));
    }
    let provider = rpc_provider(&target.rpc_url)?;
    let contract_address = parse_address(&target.contract_address, "configured token contract")?;
    let transfer_data = Erc20::new(contract_address, Arc::new(provider.clone()))
        .transfer(
            parse_address(&target.destination_address, "stored withdrawal destination")?,
            usd_micros_to_token_units(target.amount_usd_micros, target.token_decimals)?,
        )
        .calldata()
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not encode withdrawal transaction",
            )
        })?;
    let nonce = provider
        .get_transaction_count(
            wallet.address(),
            Some(BlockId::Number(BlockNumber::Pending)),
        )
        .await
        .map_err(chain_error)?;
    let gas_price = provider.get_gas_price().await.map_err(chain_error)?;
    let mut transaction: TypedTransaction = TransactionRequest::new()
        .from(wallet.address())
        .to(contract_address)
        .data(transfer_data)
        .nonce(nonce)
        .gas_price(gas_price)
        .chain_id(target.chain_id as u64)
        .into();
    let gas = provider
        .estimate_gas(&transaction, None)
        .await
        .map_err(chain_error)?;
    transaction.set_gas(gas);
    let signature = wallet
        .sign_transaction(&transaction)
        .await
        .map_err(chain_error)?;
    let signed_transaction = transaction.rlp_signed(&signature);
    let transaction_hash = H256::from(keccak256(signed_transaction.as_ref()));
    Ok((provider, signed_transaction, transaction_hash))
}

async fn persist_signed_withdrawal(
    db: &SqlitePool,
    withdrawal_id: &str,
    transaction_hash: H256,
    signed_transaction: &Bytes,
) -> Result<(), ApiError> {
    let now = Utc::now().to_rfc3339();
    let mut tx = db.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE withdrawals SET transaction_hash=?1,signed_transaction=?2,last_error=NULL,status='submitted',updated_at=?3 WHERE id=?4 AND transaction_hash IS NULL AND signed_transaction IS NULL")
        .bind(format!("{transaction_hash:#x}"))
        .bind(format!("0x{}", hex::encode(signed_transaction.as_ref())))
        .bind(&now)
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE payment_operations SET status='submitted',updated_at=?1 WHERE kind='withdrawal' AND resource_id=?2")
        .bind(&now)
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

async fn record_withdrawal_error(
    state: &AppState,
    withdrawal_id: &str,
    error_message: &str,
) -> Result<(), ApiError> {
    let _write = state.write_lock.lock().await;
    let now = Utc::now().to_rfc3339();
    let row = sqlx::query("SELECT status,signed_transaction FROM withdrawals WHERE id=?1")
        .bind(withdrawal_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_error)?;
    let Some(row) = row else {
        return Ok(());
    };
    let status: String = row.get(0);
    if !matches!(status.as_str(), "awaiting_signer" | "submitted") {
        return Ok(());
    }
    let signed_transaction: Option<String> = row.get(1);
    let operation_status = if signed_transaction.is_some() {
        "submitted"
    } else {
        "failed"
    };
    let mut tx = state.db.begin().await.map_err(db_error)?;
    sqlx::query("UPDATE withdrawals SET last_error=?1,updated_at=?2 WHERE id=?3 AND status IN ('awaiting_signer','submitted')")
        .bind(error_message)
        .bind(&now)
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("UPDATE payment_operations SET status=?1,updated_at=?2 WHERE kind='withdrawal' AND resource_id=?3 AND EXISTS(SELECT 1 FROM withdrawals WHERE id=?3 AND status IN ('awaiting_signer','submitted'))")
        .bind(operation_status)
        .bind(&now)
        .bind(withdrawal_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

fn signed_raw_transaction(value: &str) -> Result<Bytes, ApiError> {
    let value = value.strip_prefix("0x").unwrap_or(value);
    if value.is_empty() {
        return Err(ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored withdrawal signed transaction is invalid",
        ));
    }
    hex::decode(value).map(Bytes::from).map_err(|_| {
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored withdrawal signed transaction is invalid",
        )
    })
}

fn withdrawal_is_retryable(
    status: &str,
    transaction_hash: Option<&str>,
    signed_transaction: Option<&str>,
) -> bool {
    matches!(
        (
            status,
            transaction_hash.is_some(),
            signed_transaction.is_some()
        ),
        ("awaiting_signer", false, false) | ("failed", false, false) | ("submitted", true, true)
    )
}

async fn load_deposit_target(
    db: &SqlitePool,
    user_id: &str,
    input: &DepositRequest,
) -> Result<DepositTarget, ApiError> {
    let asset = builtin_asset(&input.asset_id)
        .ok_or_else(|| ApiError::invalid("the deposit asset is not supported"))?;
    let (wallet_id, address) = load_user_deposit_wallet(db, user_id).await?;
    Ok(deposit_target(wallet_id, address, asset))
}

async fn load_deposit_targets_for_chain(
    db: &SqlitePool,
    user_id: &str,
    chain_id: i64,
) -> Result<Vec<DepositTarget>, ApiError> {
    if builtin_network(chain_id).is_none() {
        return Err(ApiError::invalid("the deposit network is not supported"));
    }
    let (wallet_id, address) = load_user_deposit_wallet(db, user_id).await?;
    Ok(BUILTIN_ASSETS
        .iter()
        .filter(|asset| asset.chain_id == chain_id)
        .map(|asset| deposit_target(wallet_id.clone(), address.clone(), *asset))
        .collect())
}

async fn load_user_deposit_wallet(
    db: &SqlitePool,
    user_id: &str,
) -> Result<(String, String), ApiError> {
    let row = sqlx::query("SELECT w.id,w.address FROM wallet_addresses w JOIN wallet_private_keys k ON k.wallet_address_id=w.id WHERE w.user_id=?1 ORDER BY w.created_at,w.id LIMIT 1")
        .bind(user_id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "deposit wallet was not initialized"))?;
    Ok((row.get(0), row.get(1)))
}

fn deposit_target(wallet_id: String, address: String, asset: BuiltinAsset) -> DepositTarget {
    DepositTarget {
        wallet_id,
        address,
        asset_id: asset.id.to_string(),
        symbol: asset.symbol.to_string(),
        contract_address: asset.contract_address.to_string(),
        rpc_url: builtin_network(asset.chain_id)
            .expect("builtin asset always has a network")
            .rpc_url
            .to_string(),
        token_decimals: asset.token_decimals,
    }
}

async fn verify_deposit_receipt(
    target: &DepositTarget,
    value: &str,
) -> Result<VerifiedDeposit, ApiError> {
    let (transaction_hash, receipt) = confirmed_deposit_receipt(&target.rpc_url, value).await?;
    verified_deposit_transfer(target, transaction_hash, &receipt.logs)
}

async fn verify_deposit_claim(
    targets: Vec<DepositTarget>,
    value: &str,
) -> Result<(DepositTarget, VerifiedDeposit), ApiError> {
    let rpc_url = targets
        .first()
        .expect("each supported network has USDC and USDT")
        .rpc_url
        .clone();
    let (transaction_hash, receipt) = confirmed_deposit_receipt(&rpc_url, value).await?;
    for target in targets {
        if let Ok(verified) = verified_deposit_transfer(&target, transaction_hash, &receipt.logs) {
            return Ok((target, verified));
        }
    }
    Err(ApiError::invalid(
        "the transaction does not contain a supported USDC or USDT Transfer to this deposit address",
    ))
}

async fn confirmed_deposit_receipt(
    rpc_url: &str,
    value: &str,
) -> Result<(H256, TransactionReceipt), ApiError> {
    let transaction_hash = H256::from_str(value)
        .map_err(|_| ApiError::invalid("transaction_hash must be a 32-byte EVM hash"))?;
    let receipt = rpc_provider(rpc_url)?
        .get_transaction_receipt(transaction_hash)
        .await
        .map_err(chain_error)?
        .ok_or_else(|| ApiError::conflict("the deposit transaction is not confirmed yet"))?;
    if receipt.status.map(|status| status.as_u64()) != Some(1) {
        return Err(ApiError::invalid("the deposit transaction reverted"));
    }
    Ok((transaction_hash, receipt))
}

fn verified_deposit_transfer(
    target: &DepositTarget,
    transaction_hash: H256,
    logs: &[Log],
) -> Result<VerifiedDeposit, ApiError> {
    let contract = parse_address(&target.contract_address, "configured token contract")?;
    let recipient = parse_address(&target.address, "stored deposit address")?;
    let transfer_topic = H256::from(keccak256("Transfer(address,address,uint256)"));
    for (log_index, log) in logs.iter().enumerate() {
        if log.address != contract || log.topics.len() != 3 || log.topics[0] != transfer_topic {
            continue;
        }
        if Address::from_slice(&log.topics[2].as_bytes()[12..]) != recipient {
            continue;
        }
        let amount = U256::from_big_endian(log.data.0.as_ref());
        let amount_usd_micros = token_units_to_usd_micros(amount, target.token_decimals)?;
        return Ok(VerifiedDeposit {
            amount_usd_micros,
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
    let row = sqlx::query("SELECT w.id,a.symbol,w.destination_address,w.address_book_entry_id,b.label,w.amount_usd_micros,w.transaction_hash,w.status FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id LEFT JOIN address_book_entries b ON b.id=w.address_book_entry_id WHERE w.id=?1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("withdrawal does not exist"))?;
    let amount_usd_micros: i64 = row.get(5);
    Ok(WithdrawalResponse {
        id: row.get(0),
        asset_symbol: row.get(1),
        destination_address: row.get(2),
        address_book_entry_id: row.get(3),
        destination_label: row.get(4),
        amount_usd_micros,
        amount_usd: format_usd(amount_usd_micros),
        transaction_hash: row.get(6),
        status: row.get(7),
    })
}

async fn load_withdrawal_target(
    db: &SqlitePool,
    id: &str,
    user_id: Option<&str>,
) -> Result<WithdrawalTarget, ApiError> {
    let row = sqlx::query("SELECT a.contract_address,n.rpc_url,n.chain_id,w.destination_address,w.amount_usd_micros,a.token_decimals,w.transaction_hash,w.status FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id JOIN evm_networks n ON n.chain_id=a.chain_id WHERE w.id=?1")
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
    let signed_transaction: Option<String> =
        sqlx::query_scalar("SELECT signed_transaction FROM withdrawals WHERE id=?1")
            .bind(id)
            .fetch_one(db)
            .await
            .map_err(db_error)?;
    Ok(WithdrawalTarget {
        contract_address: row.get(0),
        rpc_url: row.get(1),
        chain_id: row.get(2),
        destination_address: row.get(3),
        amount_usd_micros: row.get(4),
        token_decimals: row.get::<i64, _>(5) as u8,
        transaction_hash: row.get(6),
        signed_transaction,
        status: row.get(7),
    })
}

async fn load_evm_config(db: &SqlitePool) -> Result<EvmConfig, ApiError> {
    Ok(EvmConfig {
        custody_wallet_address: meta(db, CUSTODY_WALLET_ADDRESS_KEY)
            .await
            .map_err(db_error)?,
        custody_wallet_private_key_configured: meta(db, CUSTODY_WALLET_PRIVATE_KEY_KEY)
            .await
            .map_err(db_error)?
            .is_some(),
        networks: builtin_networks(),
        assets: builtin_assets(),
    })
}

async fn load_custody_balances(db: &SqlitePool) -> Result<CustodyBalancesResponse, ApiError> {
    let custody_wallet_address = meta(db, CUSTODY_WALLET_ADDRESS_KEY)
        .await
        .map_err(db_error)?;
    let Some(custody_wallet_address) = custody_wallet_address else {
        return Ok(CustodyBalancesResponse {
            custody_wallet_address: None,
            networks: Vec::new(),
        });
    };
    let address = parse_address(&custody_wallet_address, "configured custody wallet")?;
    let mut jobs = JoinSet::new();
    for network in BUILTIN_EVM_NETWORKS {
        jobs.spawn(async move { load_custody_network_balances(network, address).await });
    }
    let mut networks = Vec::with_capacity(BUILTIN_EVM_NETWORKS.len());
    while let Some(job) = jobs.join_next().await {
        let network = job.map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "custody balance lookup task stopped unexpectedly",
            )
        })?;
        networks.push(network?);
    }
    networks.sort_by_key(|network| network.chain_id);
    Ok(CustodyBalancesResponse {
        custody_wallet_address: Some(custody_wallet_address),
        networks,
    })
}

async fn load_custody_network_balances(
    network: BuiltinEvmNetwork,
    custody_address: Address,
) -> Result<CustodyNetworkBalances, ApiError> {
    let provider = rpc_provider(network.rpc_url)?;
    let usdc_asset = builtin_asset_for_network(network.chain_id, "USDC");
    let usdt_asset = builtin_asset_for_network(network.chain_id, "USDT");
    let usdc_contract = parse_address(usdc_asset.contract_address, "configured token contract")?;
    let usdt_contract = parse_address(usdt_asset.contract_address, "configured token contract")?;
    let usdc = Erc20::new(usdc_contract, Arc::new(provider.clone()));
    let usdt = Erc20::new(usdt_contract, Arc::new(provider.clone()));
    let usdc_call = usdc.balance_of(custody_address);
    let usdt_call = usdt.balance_of(custody_address);
    let (native_result, usdc_result, usdt_result) = tokio::join!(
        provider.get_balance(custody_address, None),
        usdc_call.call(),
        usdt_call.call(),
    );
    Ok(CustodyNetworkBalances {
        chain_id: network.chain_id,
        network_name: network.name.to_string(),
        native: custody_asset_balance(native_asset_symbol(network.chain_id), 18, native_result),
        usdc: custody_asset_balance(usdc_asset.symbol, usdc_asset.token_decimals, usdc_result),
        usdt: custody_asset_balance(usdt_asset.symbol, usdt_asset.token_decimals, usdt_result),
    })
}

fn custody_asset_balance<E: std::fmt::Display>(
    symbol: &str,
    decimals: u8,
    result: Result<U256, E>,
) -> CustodyAssetBalance {
    match result {
        Ok(amount) => CustodyAssetBalance {
            symbol: symbol.to_string(),
            amount: Some(format_token_amount(amount, decimals)),
            error: None,
        },
        Err(error) => CustodyAssetBalance {
            symbol: symbol.to_string(),
            amount: None,
            error: Some(format!("EVM RPC operation failed: {error}")),
        },
    }
}

fn format_token_amount(amount: U256, decimals: u8) -> String {
    let scale = U256::exp10(decimals as usize);
    let whole = amount / scale;
    let fractional = amount % scale;
    if fractional.is_zero() {
        return whole.to_string();
    }
    let digits = fractional.to_string();
    let padded = format!("{}{}", "0".repeat(decimals as usize - digits.len()), digits);
    format!("{whole}.{}", padded.trim_end_matches('0'))
}

fn native_asset_symbol(chain_id: i64) -> &'static str {
    match chain_id {
        56 => "BNB",
        137 => "POL",
        _ => "ETH",
    }
}

fn builtin_network(chain_id: i64) -> Option<BuiltinEvmNetwork> {
    BUILTIN_EVM_NETWORKS
        .iter()
        .copied()
        .find(|network| network.chain_id == chain_id)
}

fn builtin_asset(id: &str) -> Option<BuiltinAsset> {
    BUILTIN_ASSETS.iter().copied().find(|asset| asset.id == id)
}

fn builtin_asset_for_network(chain_id: i64, symbol: &str) -> BuiltinAsset {
    BUILTIN_ASSETS
        .iter()
        .copied()
        .find(|asset| asset.chain_id == chain_id && asset.symbol == symbol)
        .expect("every builtin network has USDC and USDT")
}

fn builtin_networks() -> Vec<EvmNetwork> {
    BUILTIN_EVM_NETWORKS
        .iter()
        .map(|network| EvmNetwork {
            chain_id: network.chain_id,
            name: network.name.to_string(),
        })
        .collect()
}

fn builtin_assets() -> Vec<SupportedAsset> {
    BUILTIN_ASSETS
        .iter()
        .map(|asset| SupportedAsset {
            id: asset.id.to_string(),
            chain_id: asset.chain_id,
            symbol: asset.symbol.to_string(),
            contract_address: asset.contract_address.to_string(),
            token_decimals: asset.token_decimals,
            enabled: true,
            network_name: Some(
                builtin_network(asset.chain_id)
                    .expect("builtin asset always has a network")
                    .name
                    .to_string(),
            ),
        })
        .collect()
}

fn token_units_to_usd_micros(amount: U256, token_decimals: u8) -> Result<i64, ApiError> {
    let scale = U256::exp10((token_decimals - 6) as usize);
    if amount.is_zero() || amount % scale != U256::zero() {
        return Err(ApiError::invalid(
            "the confirmed token amount must be a positive whole USD micro amount",
        ));
    }
    let micros = amount / scale;
    if micros > U256::from(i64::MAX as u64) {
        return Err(ApiError::invalid(
            "the confirmed token amount is outside Midas's USD ledger range",
        ));
    }
    Ok(micros.as_u64() as i64)
}

fn usd_micros_to_token_units(value: i64, token_decimals: u8) -> Result<U256, ApiError> {
    if value <= 0 {
        return Err(ApiError::invalid(
            "withdrawal amount must be greater than zero",
        ));
    }
    Ok(U256::from(value as u64) * U256::exp10((token_decimals - 6) as usize))
}

fn address_book_label(value: &str) -> Result<String, ApiError> {
    let label = value.trim();
    if label.is_empty() || label.chars().count() > 80 || label.chars().any(char::is_control) {
        return Err(ApiError::invalid(
            "address book label must contain 1 to 80 visible characters",
        ));
    }
    Ok(label.to_string())
}

async fn read_address_book_entry(
    db: &SqlitePool,
    user_id: &str,
    id: &str,
) -> Result<AddressBookEntry, ApiError> {
    let row = sqlx::query("SELECT id,chain_id,label,address,created_at,updated_at FROM address_book_entries WHERE id=?1 AND user_id=?2")
        .bind(id)
        .bind(user_id)
        .fetch_optional(db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::invalid("address book entry does not exist"))?;
    address_book_entry(row)
}

fn address_book_entry(row: sqlx::sqlite::SqliteRow) -> Result<AddressBookEntry, ApiError> {
    let chain_id: i64 = row.get(1);
    let network = builtin_network(chain_id)
        .ok_or_else(|| ApiError::invalid("address book entry uses an unsupported EVM network"))?;
    Ok(AddressBookEntry {
        id: row.get(0),
        chain_id,
        chain_name: network.name.to_string(),
        label: row.get(2),
        address: row.get(3),
        created_at: row.get(4),
        updated_at: row.get(5),
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
    let user_id = require_user(state, headers).await?;
    ensure_user_wallet(state, &user_id).await?;
    Ok(user_id)
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
    if !table_has_column(db, "supported_assets", "token_decimals").await? {
        sqlx::query(
            "ALTER TABLE supported_assets ADD COLUMN token_decimals INTEGER NOT NULL DEFAULT 6",
        )
        .execute(db)
        .await?;
    }
    if !table_has_column(db, "withdrawals", "address_book_entry_id").await? {
        sqlx::query("ALTER TABLE withdrawals ADD COLUMN address_book_entry_id TEXT REFERENCES address_book_entries(id)")
            .execute(db)
            .await?;
    }
    if !table_has_column(db, "withdrawals", "signed_transaction").await? {
        sqlx::query("ALTER TABLE withdrawals ADD COLUMN signed_transaction TEXT")
            .execute(db)
            .await?;
    }
    if !table_has_column(db, "withdrawals", "last_error").await? {
        sqlx::query("ALTER TABLE withdrawals ADD COLUMN last_error TEXT")
            .execute(db)
            .await?;
    }
    sqlx::query("DELETE FROM app_meta WHERE key='etherscan_api_key'")
        .execute(db)
        .await?;
    seed_builtin_evm(db).await?;
    migrate_legacy_custody_wallet(db).await?;
    Ok(())
}

async fn table_has_column(db: &SqlitePool, table: &str, column: &str) -> anyhow::Result<bool> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(db)
        .await?;
    Ok(rows
        .iter()
        .any(|row| row.get::<String, _>("name") == column))
}

async fn seed_builtin_evm(db: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query("UPDATE supported_assets SET enabled=0")
        .execute(db)
        .await?;
    sqlx::query("UPDATE evm_networks SET enabled=0")
        .execute(db)
        .await?;
    for network in BUILTIN_EVM_NETWORKS {
        sqlx::query("INSERT INTO evm_networks(chain_id,name,rpc_url,enabled) VALUES(?1,?2,?3,1) ON CONFLICT(chain_id) DO UPDATE SET name=excluded.name,rpc_url=excluded.rpc_url,enabled=1")
            .bind(network.chain_id)
            .bind(network.name)
            .bind(network.rpc_url)
            .execute(db)
            .await?;
    }
    for asset in BUILTIN_ASSETS {
        sqlx::query("INSERT INTO supported_assets(id,chain_id,symbol,contract_address,token_decimals,enabled) VALUES(?1,?2,?3,?4,?5,1) ON CONFLICT(id) DO UPDATE SET chain_id=excluded.chain_id,symbol=excluded.symbol,contract_address=excluded.contract_address,token_decimals=excluded.token_decimals,enabled=1")
            .bind(asset.id)
            .bind(asset.chain_id)
            .bind(asset.symbol)
            .bind(asset.contract_address)
            .bind(asset.token_decimals as i64)
            .execute(db)
            .await?;
    }
    Ok(())
}

async fn migrate_legacy_custody_wallet(db: &SqlitePool) -> anyhow::Result<()> {
    if meta(db, CUSTODY_WALLET_PRIVATE_KEY_KEY).await?.is_some() {
        return Ok(());
    }
    let legacy_key = meta(db, LEGACY_COLLECTION_WALLET_PRIVATE_KEY_KEY)
        .await?
        .or(meta(db, LEGACY_GAS_ACCOUNT_PRIVATE_KEY_KEY).await?);
    let Some(legacy_key) = legacy_key else {
        return Ok(());
    };
    let wallet = legacy_key.parse::<LocalWallet>()?;
    set_meta(db, CUSTODY_WALLET_PRIVATE_KEY_KEY, &legacy_key).await?;
    set_meta(
        db,
        CUSTODY_WALLET_ADDRESS_KEY,
        &format!("{:#x}", wallet.address()),
    )
    .await?;
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
        let cursor_table: String = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='deposit_discovery_cursors'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(cursor_table, "deposit_discovery_cursors");
        let deposit_transaction_index: String = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='deposits_transaction_hash_unique_idx'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            deposit_transaction_index,
            "deposits_transaction_hash_unique_idx"
        );
        let address_book_table: String = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='address_book_entries'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(address_book_table, "address_book_entries");
        assert!(
            table_has_column(&db, "withdrawals", "signed_transaction")
                .await
                .unwrap()
        );
        assert!(
            table_has_column(&db, "withdrawals", "last_error")
                .await
                .unwrap()
        );
        let builtin_assets: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM supported_assets WHERE enabled=1")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(builtin_assets, 12);
        let bsc_rpc_url: String =
            sqlx::query_scalar("SELECT rpc_url FROM evm_networks WHERE chain_id=56")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(bsc_rpc_url, "https://bsc-dataseed.binance.org/");
        let optimism_rpc_url: String =
            sqlx::query_scalar("SELECT rpc_url FROM evm_networks WHERE chain_id=10")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(optimism_rpc_url, "https://mainnet.optimism.io");
        let arbitrum_rpc_url: String =
            sqlx::query_scalar("SELECT rpc_url FROM evm_networks WHERE chain_id=42161")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(arbitrum_rpc_url, "https://arb1.arbitrum.io/rpc");
        set_meta(&db, "etherscan_api_key", "obsolete-key")
            .await
            .unwrap();
        migrate(&db).await.unwrap();
        assert!(meta(&db, "etherscan_api_key").await.unwrap().is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn formats_usd_micros() {
        assert_eq!(format_usd(1_250_000), "1.250000");
        assert_eq!(format_usd(-1), "-0.000001");
    }

    #[test]
    fn bounds_rpc_discovery_and_resumes_from_the_saved_cursor() {
        assert_eq!(rpc_discovery_range(0, 5_000), Some((3_977, 4_026)));
        assert_eq!(rpc_discovery_range(101, 5_000), Some((101, 150)));
        assert_eq!(rpc_discovery_range(5_001, 5_000), None);
        assert_eq!(
            discovery_rpc_url(builtin_network(1).unwrap()),
            ETHEREUM_DISCOVERY_RPC_URL
        );
        assert_eq!(
            discovery_rpc_url(builtin_network(56).unwrap()),
            BSC_DISCOVERY_RPC_URL
        );
        assert_eq!(
            discovery_rpc_url(builtin_network(8453).unwrap()),
            "https://mainnet.base.org"
        );
    }

    #[tokio::test]
    async fn automatically_discovered_deposit_is_idempotent_by_verified_transfer_event() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        let wallet_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let address = "0x0000000000000000000000000000000000000001";
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO wallet_addresses(id,user_id,chain_id,address,created_at) VALUES(?1,?2,1,?3,?4)")
            .bind(&wallet_id)
            .bind(&user_id)
            .bind(address)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        let target = DepositTarget {
            wallet_id,
            address: address.to_string(),
            asset_id: "1-USDC".to_string(),
            symbol: "USDC".to_string(),
            contract_address: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
            rpc_url: builtin_network(1).unwrap().rpc_url.to_string(),
            token_decimals: 6,
        };
        let verified = VerifiedDeposit {
            amount_usd_micros: 2_000_000,
            raw_amount: "2000000".to_string(),
            transaction_hash: format!("0x{}", "a".repeat(64)),
            log_index: 4,
        };
        let idempotency_key = "rpc:1-USDC:verified-transfer:4";
        assert!(
            insert_verified_deposit(
                &db,
                &user_id,
                &target,
                &verified,
                idempotency_key,
                "Automatically discovered EVM deposit",
            )
            .await
            .unwrap()
            .is_some()
        );
        assert!(
            insert_verified_deposit(
                &db,
                &user_id,
                &target,
                &verified,
                idempotency_key,
                "Automatically discovered EVM deposit",
            )
            .await
            .unwrap()
            .is_none()
        );
        let second_event_in_same_transaction = VerifiedDeposit {
            log_index: 5,
            ..verified
        };
        assert!(
            insert_verified_deposit(
                &db,
                &user_id,
                &target,
                &second_event_in_same_transaction,
                "rpc:1-USDC:verified-transfer:5",
                "Automatically discovered EVM deposit",
            )
            .await
            .unwrap()
            .is_none()
        );
        let deposits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM deposits")
            .fetch_one(&db)
            .await
            .unwrap();
        let operations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM payment_operations WHERE kind='deposit'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(deposits, 1);
        assert_eq!(operations, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn formats_native_and_token_balances_without_float_rounding() {
        assert_eq!(format_token_amount(U256::zero(), 18), "0");
        assert_eq!(format_token_amount(U256::from(1_250_000_u64), 6), "1.25");
        assert_eq!(
            format_token_amount(U256::from(1_u64), 18),
            "0.000000000000000001"
        );
        assert_eq!(native_asset_symbol(1), "ETH");
        assert_eq!(native_asset_symbol(56), "BNB");
        assert_eq!(native_asset_symbol(137), "POL");
    }

    #[test]
    fn custody_asset_balance_keeps_a_per_asset_rpc_error_visible() {
        let balance = custody_asset_balance("USDC", 6, Err::<U256, _>("offline RPC"));
        assert_eq!(balance.symbol, "USDC");
        assert!(balance.amount.is_none());
        assert!(balance.error.unwrap().contains("offline RPC"));
    }

    #[tokio::test]
    async fn custody_balance_lookup_without_a_configured_wallet_skips_rpc_calls() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();

        let balances = load_custody_balances(&db).await.unwrap();

        assert!(balances.custody_wallet_address.is_none());
        assert!(balances.networks.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn only_unsubmitted_sweeps_are_retryable() {
        assert!(sweep_is_retryable("queued"));
        assert!(sweep_is_retryable("awaiting_configuration"));
        assert!(sweep_is_retryable("failed"));
        assert!(!sweep_is_retryable("submitted"));
        assert!(!sweep_is_retryable("swept"));
    }

    #[test]
    fn withdrawal_retry_never_resigns_a_transaction_with_an_existing_hash() {
        assert!(withdrawal_is_retryable("awaiting_signer", None, None));
        assert!(withdrawal_is_retryable("failed", None, None));
        assert!(withdrawal_is_retryable(
            "submitted",
            Some("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some("0x01")
        ));
        assert!(!withdrawal_is_retryable(
            "submitted",
            Some("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            None
        ));
        assert!(!withdrawal_is_retryable(
            "failed",
            Some("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            None
        ));
        assert!(!withdrawal_is_retryable("completed", None, None));
    }

    #[tokio::test]
    async fn signed_withdrawal_is_durable_before_the_broadcast_boundary() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        let withdrawal_id = Uuid::new_v4().to_string();
        let ledger_id = Uuid::new_v4().to_string();
        let operation_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,asset_id,amount_usd_micros,balance_delta_usd_micros,created_at) VALUES(?1,?2,'withdrawal','pending','56-USDT',1000000,-1000000,?3)")
            .bind(&ledger_id)
            .bind(&user_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO withdrawals(id,user_id,asset_id,ledger_entry_id,destination_address,amount_usd_micros,status,created_at,updated_at) VALUES(?1,?2,'56-USDT',?3,'0x0000000000000000000000000000000000000001',1000000,'awaiting_signer',?4,?4)")
            .bind(&withdrawal_id)
            .bind(&user_id)
            .bind(&ledger_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO payment_operations(id,user_id,kind,idempotency_key,resource_id,status,created_at,updated_at) VALUES(?1,?2,'withdrawal','test-withdrawal',?3,'accepted',?4,?4)")
            .bind(&operation_id)
            .bind(&user_id)
            .bind(&withdrawal_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        let raw = Bytes::from(vec![1_u8, 2, 3, 4]);
        let hash = H256::from(keccak256(raw.as_ref()));

        persist_signed_withdrawal(&db, &withdrawal_id, hash, &raw)
            .await
            .unwrap();

        let row = sqlx::query("SELECT transaction_hash,signed_transaction,status,last_error FROM withdrawals WHERE id=?1")
            .bind(&withdrawal_id)
            .fetch_one(&db)
            .await
            .unwrap();
        let transaction_hash: Option<String> = row.get(0);
        let signed_transaction: Option<String> = row.get(1);
        let status: String = row.get(2);
        let last_error: Option<String> = row.get(3);
        let expected_hash = format!("{hash:#x}");
        assert_eq!(transaction_hash.as_deref(), Some(expected_hash.as_str()));
        assert_eq!(signed_transaction.as_deref(), Some("0x01020304"));
        assert_eq!(status, "submitted");
        assert!(last_error.is_none());
        assert!(withdrawal_is_retryable(
            &status,
            transaction_hash.as_deref(),
            signed_transaction.as_deref()
        ));
        let operation_status: String =
            sqlx::query_scalar("SELECT status FROM payment_operations WHERE id=?1")
                .bind(&operation_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(operation_status, "submitted");

        let settlement_lock = Mutex::new(());
        settle_withdrawal(&db, &settlement_lock, &withdrawal_id, true)
            .await
            .unwrap();
        let withdrawal_status: String =
            sqlx::query_scalar("SELECT status FROM withdrawals WHERE id=?1")
                .bind(&withdrawal_id)
                .fetch_one(&db)
                .await
                .unwrap();
        let ledger_status: String =
            sqlx::query_scalar("SELECT status FROM ledger_entries WHERE id=?1")
                .bind(&ledger_id)
                .fetch_one(&db)
                .await
                .unwrap();
        let operation_status: String =
            sqlx::query_scalar("SELECT status FROM payment_operations WHERE id=?1")
                .bind(&operation_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(withdrawal_status, "completed");
        assert_eq!(ledger_status, "posted");
        assert_eq!(operation_status, "completed");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn evm_private_keys_generate_addresses() {
        let wallet = LocalWallet::new(&mut thread_rng());
        assert!(format!("{:#x}", wallet.address()).starts_with("0x"));
    }

    #[tokio::test]
    async fn each_user_gets_one_private_evm_wallet_without_a_chain_api_field() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user_id)
            .execute(&db)
            .await
            .unwrap();

        let first = provision_user_wallet(&db, &user_id).await.unwrap();
        let second = provision_user_wallet(&db, &user_id).await.unwrap();
        assert_eq!(first.address, second.address);
        assert!(
            serde_json::to_value(first)
                .unwrap()
                .get("chain_id")
                .is_none()
        );
        let address_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM wallet_addresses WHERE user_id=?1")
                .bind(&user_id)
                .fetch_one(&db)
                .await
                .unwrap();
        let key_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM wallet_private_keys WHERE wallet_address_id IN (SELECT id FROM wallet_addresses WHERE user_id=?1)",
        )
        .bind(&user_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(address_count, 1);
        assert_eq!(key_count, 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn transfer_provisions_a_new_recipient_wallet_and_posts_their_ledger_entry() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let sender = Uuid::new_v4().to_string();
        let recipient = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&sender)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,amount_usd_micros,balance_delta_usd_micros,created_at) VALUES(?1,?2,'adjustment','posted',2000000,2000000,?3)")
            .bind(Uuid::new_v4().to_string())
            .bind(&sender)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();

        let request = TransferRequest {
            recipient_user_id: recipient.clone(),
            amount_usd_micros: 750_000,
            note: Some("Wallet creation transfer".to_string()),
        };
        let transfer = post_transfer(&db, &sender, &request, "new-recipient")
            .await
            .unwrap();
        let retry = post_transfer(&db, &sender, &request, "new-recipient")
            .await
            .unwrap();

        assert_eq!(transfer.id, retry.id);
        assert_eq!(transfer.recipient_user_id, recipient);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE id=?1")
                .bind(&recipient)
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM wallet_addresses WHERE user_id=?1")
                .bind(&recipient)
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM wallet_private_keys WHERE wallet_address_id IN (SELECT id FROM wallet_addresses WHERE user_id=?1)")
                .bind(&recipient)
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT balance_delta_usd_micros FROM ledger_entries WHERE user_id=?1 AND kind='transfer_in'")
                .bind(&recipient)
                .fetch_one(&db)
                .await
                .unwrap(),
            750_000
        );
        assert_eq!(available_balance(&db, &sender).await.unwrap(), 1_250_000);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM internal_transfers")
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn withdrawal_requests_use_a_direct_destination_address() {
        let request: WithdrawalRequest = serde_json::from_value(serde_json::json!({
            "asset_id": "1-USDC",
            "destination_address": "0x0000000000000000000000000000000000000001",
            "amount_usd_micros": 1_000_000,
        }))
        .unwrap();
        assert_eq!(
            request.destination_address,
            "0x0000000000000000000000000000000000000001"
        );
    }

    #[tokio::test]
    async fn a_withdrawal_destination_can_be_saved_once_and_reused() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        let withdrawal_id = Uuid::new_v4().to_string();
        let ledger_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let address = "0x0000000000000000000000000000000000000001";
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,asset_id,amount_usd_micros,balance_delta_usd_micros,created_at) VALUES(?1,?2,'withdrawal','pending','1-USDC',1000000,-1000000,?3)")
            .bind(&ledger_id)
            .bind(&user_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO withdrawals(id,user_id,asset_id,ledger_entry_id,destination_address,amount_usd_micros,status,created_at,updated_at) VALUES(?1,?2,'1-USDC',?3,?4,1000000,'awaiting_signer',?5,?5)")
            .bind(&withdrawal_id)
            .bind(&user_id)
            .bind(&ledger_id)
            .bind(address)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();

        let saved = save_withdrawal_destination_for_user(&db, &user_id, &withdrawal_id, "Treasury")
            .await
            .unwrap();
        let reused = save_withdrawal_destination_for_user(&db, &user_id, &withdrawal_id, "Ignored")
            .await
            .unwrap();
        assert_eq!(saved.id, reused.id);
        assert_eq!(saved.label, "Treasury");
        assert_eq!(saved.chain_id, 1);
        assert_eq!(saved.address, address);
        let linked_entry_id: Option<String> = sqlx::query_scalar(
            "SELECT address_book_entry_id FROM withdrawals WHERE id=?1 AND user_id=?2",
        )
        .bind(&withdrawal_id)
        .bind(&user_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(linked_entry_id.as_deref(), Some(saved.id.as_str()));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn builtin_evm_assets_cover_common_networks_with_exact_usd_scaling() {
        assert_eq!(BUILTIN_EVM_NETWORKS.len(), 6);
        assert_eq!(BUILTIN_ASSETS.len(), 12);
        assert_eq!(
            builtin_network(8453).unwrap().rpc_url,
            "https://mainnet.base.org"
        );
        assert_eq!(builtin_asset("1-USDC").unwrap().token_decimals, 6);
        assert_eq!(builtin_asset("56-USDC").unwrap().token_decimals, 18);
        assert_eq!(builtin_asset("8453-USDT").unwrap().chain_id, 8453);
        assert_eq!(
            token_units_to_usd_micros(U256::exp10(18), 18).unwrap(),
            1_000_000
        );
        assert_eq!(
            usd_micros_to_token_units(1_000_000, 18).unwrap(),
            U256::exp10(18)
        );
        assert!(token_units_to_usd_micros(U256::from(1_u64), 18).is_err());
    }

    #[test]
    fn credits_the_reported_bsc_usdt_transfer_from_its_receipt_log() {
        let contract = Address::from_str("0x55d398326f99059ff775485246999027b3197955").unwrap();
        let sender = Address::from_str("0xeb2d2f1b8c558a40207669291fda468e50c8a0bb").unwrap();
        let recipient = Address::from_str("0x70166492386b11f30ad2db285f103cfd56b7e990").unwrap();
        let mut sender_topic = [0_u8; 32];
        sender_topic[12..].copy_from_slice(sender.as_bytes());
        let mut recipient_topic = [0_u8; 32];
        recipient_topic[12..].copy_from_slice(recipient.as_bytes());
        let amount = U256::from_dec_str("2990000000000000000").unwrap();
        let mut data = [0_u8; 32];
        amount.to_big_endian(&mut data);
        let target = DepositTarget {
            wallet_id: "wallet".to_string(),
            address: format!("{recipient:#x}"),
            asset_id: "56-USDT".to_string(),
            symbol: "USDT".to_string(),
            contract_address: format!("{contract:#x}"),
            rpc_url: builtin_network(56).unwrap().rpc_url.to_string(),
            token_decimals: 18,
        };
        let log = Log {
            address: contract,
            topics: vec![
                H256::from(keccak256("Transfer(address,address,uint256)")),
                H256::from(sender_topic),
                H256::from(recipient_topic),
            ],
            data: data.to_vec().into(),
            ..Default::default()
        };

        let verified = verified_deposit_transfer(
            &target,
            H256::from_str("0xe20c5cd0f743181614128a32c6e982c11355190f70e4e580d71207d914f23abe")
                .unwrap(),
            &[log],
        )
        .unwrap();
        assert_eq!(verified.amount_usd_micros, 2_990_000);
        assert_eq!(verified.raw_amount, "2990000000000000000");
        assert_eq!(
            builtin_network(56).unwrap().rpc_url,
            "https://bsc-dataseed.binance.org/"
        );
    }

    #[tokio::test]
    async fn legacy_custody_key_is_migrated_to_the_single_wallet_setting() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let wallet = LocalWallet::new(&mut thread_rng());
        let key = format!("0x{}", hex::encode(wallet.signer().to_bytes()));
        set_meta(&db, LEGACY_COLLECTION_WALLET_PRIVATE_KEY_KEY, &key)
            .await
            .unwrap();
        migrate_legacy_custody_wallet(&db).await.unwrap();
        assert_eq!(
            meta(&db, CUSTODY_WALLET_ADDRESS_KEY).await.unwrap(),
            Some(format!("{:#x}", wallet.address()))
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn migration_upgrades_the_previous_asset_and_withdrawal_schema() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        sqlx::query("CREATE TABLE evm_networks (chain_id INTEGER PRIMARY KEY, name TEXT NOT NULL, rpc_url TEXT NOT NULL, enabled INTEGER NOT NULL DEFAULT 0)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE supported_assets (id TEXT PRIMARY KEY, chain_id INTEGER NOT NULL, symbol TEXT NOT NULL, contract_address TEXT NOT NULL, usd_scale INTEGER NOT NULL DEFAULT 6, enabled INTEGER NOT NULL DEFAULT 0, UNIQUE(chain_id, symbol))")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE withdrawals (id TEXT PRIMARY KEY, user_id TEXT NOT NULL, asset_id TEXT NOT NULL, ledger_entry_id TEXT NOT NULL, destination_address TEXT NOT NULL, amount_usd_micros INTEGER NOT NULL, transaction_hash TEXT, status TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)")
            .execute(&db)
            .await
            .unwrap();
        migrate(&db).await.unwrap();
        assert!(
            table_has_column(&db, "supported_assets", "token_decimals")
                .await
                .unwrap()
        );
        assert!(
            table_has_column(&db, "withdrawals", "address_book_entry_id")
                .await
                .unwrap()
        );
        assert!(
            table_has_column(&db, "withdrawals", "signed_transaction")
                .await
                .unwrap()
        );
        assert!(
            table_has_column(&db, "withdrawals", "last_error")
                .await
                .unwrap()
        );
        let bsc_decimals: i64 =
            sqlx::query_scalar("SELECT token_decimals FROM supported_assets WHERE id='56-USDC'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(bsc_decimals, 18);
        let _ = std::fs::remove_file(path);
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
    async fn admin_reports_balances_and_chain_transaction_metadata_for_ledger_entries() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        let counterparty_user_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO users(id) VALUES(?1),(?2)")
            .bind(&user_id)
            .bind(&counterparty_user_id)
            .execute(&db)
            .await
            .unwrap();
        provision_user_wallet(&db, &user_id).await.unwrap();
        let wallet_id: String =
            sqlx::query_scalar("SELECT id FROM wallet_addresses WHERE user_id=?1")
                .bind(&user_id)
                .fetch_one(&db)
                .await
                .unwrap();
        let deposit_ledger_id = Uuid::new_v4().to_string();
        let withdrawal_ledger_id = Uuid::new_v4().to_string();
        let transfer_ledger_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,asset_id,amount_usd_micros,balance_delta_usd_micros,counterparty_user_id,external_reference,created_at) VALUES(?1,?2,'deposit','posted','56-USDT',2500000,2500000,NULL,'deposit-reference',?3),(?4,?2,'withdrawal','posted','1-USDC',750000,-750000,NULL,'withdrawal-reference',?3),(?5,?2,'transfer_out','posted',NULL,200000,-200000,?6,'transfer-reference',?3)")
            .bind(&deposit_ledger_id)
            .bind(&user_id)
            .bind(&now)
            .bind(&withdrawal_ledger_id)
            .bind(&transfer_ledger_id)
            .bind(&counterparty_user_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO deposits(id,user_id,wallet_address_id,asset_id,ledger_entry_id,transaction_hash,log_index,raw_amount,sweep_status,created_at) VALUES(?1,?2,?3,'56-USDT',?4,'0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',0,'2500000000000000000','queued',?5)")
            .bind(Uuid::new_v4().to_string())
            .bind(&user_id)
            .bind(&wallet_id)
            .bind(&deposit_ledger_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO withdrawals(id,user_id,asset_id,ledger_entry_id,destination_address,amount_usd_micros,transaction_hash,status,created_at,updated_at) VALUES(?1,?2,'1-USDC',?3,'0x0000000000000000000000000000000000000001',750000,'0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb','completed',?4,?4)")
            .bind(Uuid::new_v4().to_string())
            .bind(&user_id)
            .bind(&withdrawal_ledger_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();

        let balances = load_admin_balances(&db).await.unwrap();
        assert_eq!(balances.summary.user_count, 2);
        assert_eq!(balances.summary.funded_user_count, 1);
        assert_eq!(balances.summary.total_available_usd_micros, 1_550_000);
        assert_eq!(balances.users[0].user_id, user_id);
        let personal_ledger = load_user_ledger(&db, &user_id).await.unwrap();
        let personal_deposit = personal_ledger
            .iter()
            .find(|entry| entry.kind == "deposit")
            .unwrap();
        assert_eq!(
            personal_deposit.network_name.as_deref(),
            Some("BNB Smart Chain")
        );
        assert!(personal_deposit.transaction_hash.is_some());
        assert!(
            personal_ledger
                .iter()
                .find(|entry| entry.kind == "transfer_out")
                .unwrap()
                .transaction_hash
                .is_none()
        );
        let ledger = load_admin_ledger(
            &db,
            admin_ledger_filters(AdminLedgerQuery::default()).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(ledger.total, 3);
        let deposit = ledger
            .entries
            .iter()
            .find(|entry| entry.kind == "deposit")
            .unwrap();
        assert_eq!(deposit.network_name.as_deref(), Some("BNB Smart Chain"));
        assert_eq!(deposit.asset_symbol.as_deref(), Some("USDT"));
        assert!(deposit.transaction_hash.is_some());
        let withdrawal = ledger
            .entries
            .iter()
            .find(|entry| entry.kind == "withdrawal")
            .unwrap();
        assert_eq!(withdrawal.network_name.as_deref(), Some("Ethereum"));
        assert_eq!(withdrawal.asset_symbol.as_deref(), Some("USDC"));
        assert!(withdrawal.transaction_hash.is_some());
        let transfer = ledger
            .entries
            .iter()
            .find(|entry| entry.kind == "transfer_out")
            .unwrap();
        assert!(transfer.network_name.is_none());
        assert!(transfer.transaction_hash.is_none());
        let first_page = load_admin_ledger(
            &db,
            admin_ledger_filters(AdminLedgerQuery {
                limit: Some(1),
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .unwrap();
        let second_page = load_admin_ledger(
            &db,
            admin_ledger_filters(AdminLedgerQuery {
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(first_page.total, 3);
        assert_ne!(first_page.entries[0].id, second_page.entries[0].id);
        let deposits = load_admin_ledger(
            &db,
            admin_ledger_filters(AdminLedgerQuery {
                kind: Some("deposit".to_string()),
                limit: Some(1),
                ..Default::default()
            })
            .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(deposits.total, 1);
        assert_eq!(deposits.entries.len(), 1);
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

    #[tokio::test]
    async fn withdrawal_targets_are_distinct_per_asset_and_keep_their_note() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let user_id = Uuid::new_v4().to_string();
        let address = "0x0000000000000000000000000000000000000001";
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user_id)
            .execute(&db)
            .await
            .unwrap();
        for asset_id in ["1-USDC", "1-USDT"] {
            let ledger_id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,asset_id,amount_usd_micros,balance_delta_usd_micros,created_at) VALUES(?1,?2,'withdrawal','posted',?3,1000000,-1000000,?4)")
                .bind(&ledger_id)
                .bind(&user_id)
                .bind(asset_id)
                .bind(&now)
                .execute(&db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO withdrawals(id,user_id,asset_id,ledger_entry_id,destination_address,amount_usd_micros,transaction_hash,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,1000000,?6,'completed',?7,?7)")
                .bind(Uuid::new_v4().to_string())
                .bind(&user_id)
                .bind(asset_id)
                .bind(&ledger_id)
                .bind(address)
                .bind(format!("0x{}", Uuid::new_v4().simple()))
                .bind(&now)
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO withdrawal_target_notes(id,user_id,asset_id,address,label,created_at,updated_at) VALUES(?1,?2,'1-USDC',?3,'Treasury USDC',?4,?4)")
            .bind(Uuid::new_v4().to_string())
            .bind(&user_id)
            .bind(address)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        let targets = load_withdrawal_targets(&db, &user_id).await.unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(
            targets
                .iter()
                .find(|target| target.asset_id == "1-USDC")
                .unwrap()
                .label
                .as_deref(),
            Some("Treasury USDC")
        );
        assert!(
            targets
                .iter()
                .find(|target| target.asset_id == "1-USDT")
                .unwrap()
                .label
                .is_none()
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn automatic_payment_charge_is_idempotent_and_posts_a_balanced_pair() {
        let path = std::env::temp_dir().join(format!("midas-{}.sqlite3", Uuid::new_v4()));
        let db = open_db(&path).await.unwrap();
        migrate(&db).await.unwrap();
        let owner_user_id = Uuid::new_v4().to_string();
        let payer_user_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO users(id) VALUES(?1),(?2)")
            .bind(&owner_user_id)
            .bind(&payer_user_id)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,amount_usd_micros,balance_delta_usd_micros,created_at) VALUES(?1,?2,'adjustment','posted',2000000,2000000,?3)")
            .bind(Uuid::new_v4().to_string())
            .bind(&payer_user_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        let agreement = PaymentAgreement {
            id: Uuid::new_v4().to_string(),
            owner_user_id: owner_user_id.clone(),
            name: "1Exchange".to_string(),
            api_key_prefix: "midas_agreement_test".to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        let api_key = new_payment_agreement_api_key();
        sqlx::query("INSERT INTO payment_agreements(id,owner_user_id,name,api_key_hash,api_key_prefix,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)")
            .bind(&agreement.id)
            .bind(&agreement.owner_user_id)
            .bind(&agreement.name)
            .bind(payment_agreement_api_key_hash(&api_key))
            .bind(&agreement.api_key_prefix)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO payment_agreement_bindings(agreement_id,user_id,created_at) VALUES(?1,?2,?3)")
            .bind(&agreement.id)
            .bind(&payer_user_id)
            .bind(&now)
            .execute(&db)
            .await
            .unwrap();
        let charge = post_payment_agreement_charge(
            &db,
            &agreement,
            &payer_user_id,
            750_000,
            "billing-cycle-2026-09",
        )
        .await
        .unwrap();
        let retry = post_payment_agreement_charge(
            &db,
            &agreement,
            &payer_user_id,
            750_000,
            "billing-cycle-2026-09",
        )
        .await
        .unwrap();
        assert_eq!(charge.id, retry.id);
        assert_eq!(
            available_balance(&db, &payer_user_id).await.unwrap(),
            1_250_000
        );
        assert_eq!(
            available_balance(&db, &owner_user_id).await.unwrap(),
            750_000
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM payment_agreement_charges")
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM ledger_entries WHERE external_reference LIKE 'agreement:%'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            2
        );
        sqlx::query("DELETE FROM payment_agreement_bindings WHERE agreement_id=?1 AND user_id=?2")
            .bind(&agreement.id)
            .bind(&payer_user_id)
            .execute(&db)
            .await
            .unwrap();
        let rejected = post_payment_agreement_charge(
            &db,
            &agreement,
            &payer_user_id,
            1,
            "billing-cycle-2026-10",
        )
        .await
        .unwrap_err();
        assert_eq!(rejected.status, StatusCode::CONFLICT);
        let _ = std::fs::remove_file(path);
    }
}
