use super::*;
use std::time::Instant;

const CACHE_TTL: Duration = Duration::from_secs(5);
const RPC_TIMEOUT: Duration = Duration::from_secs(8);
// ASSUMPTION: 200k gas and twice the quoted price cover built-in token transfers.
// Signing enforces this budget; price spikes leave the existing reservation pending.
const TRANSFER_GAS_BUDGET: u64 = 200_000;

#[derive(Clone)]
pub(crate) struct Observation {
    token: U256,
    native: U256,
    fee: U256,
    observed_at: String,
}

#[derive(Default)]
pub(crate) struct Cache {
    entries: HashMap<String, (Instant, Result<Observation, &'static str>)>,
}

impl Cache {
    pub(crate) fn invalidate(&mut self) {
        self.entries.clear();
    }
}

#[derive(Serialize)]
pub(crate) struct Availability {
    asset_id: String,
    chain_id: i64,
    network_name: String,
    asset_symbol: String,
    available_amount: String,
    available_usd_nanos: String,
    status: &'static str,
    gas_sufficient: Option<bool>,
    observed_at: Option<String>,
    reason: Option<&'static str>,
}

#[derive(Deserialize)]
pub(crate) struct AvailabilityQuery {
    asset_id: String,
}

pub(crate) async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AvailabilityQuery>,
) -> Result<Json<Availability>, ApiError> {
    require_initialized_user(&state, &headers).await?;
    let asset =
        builtin_asset(&query.asset_id).ok_or_else(|| ApiError::invalid("unsupported asset"))?;
    // INVARIANT: all reservation mutations and cache invalidations use write_lock.
    let _write = state.write_lock.lock().await;
    let observation = cached_observation(&state.db, &state.liquidity_cache, asset).await;
    Ok(Json(availability(&state.db, asset, observation).await?))
}

async fn cached_observation(
    db: &SqlitePool,
    cache: &Mutex<Cache>,
    asset: BuiltinAsset,
) -> Result<Observation, &'static str> {
    let mut cache = cache.lock().await;
    cache.entries.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
    if let Some((_, observation)) = cache.entries.get(asset.id) {
        return observation.clone();
    }
    let observation = observe(db, asset).await;
    // Keys are restricted to the fixed built-in asset map (12 entries).
    cache
        .entries
        .insert(asset.id.to_string(), (Instant::now(), observation.clone()));
    observation
}

async fn observe(db: &SqlitePool, asset: BuiltinAsset) -> Result<Observation, &'static str> {
    match time::timeout(RPC_TIMEOUT, observe_rpc(db, asset)).await {
        Ok(result) => result,
        Err(_) => Err("rpc_unavailable"),
    }
}

async fn observe_rpc(db: &SqlitePool, asset: BuiltinAsset) -> Result<Observation, &'static str> {
    let address = meta(db, CUSTODY_WALLET_ADDRESS_KEY)
        .await
        .map_err(|_| "rpc_unavailable")?
        .ok_or("not_configured")?;
    if meta(db, CUSTODY_WALLET_PRIVATE_KEY_KEY)
        .await
        .map_err(|_| "rpc_unavailable")?
        .is_none()
    {
        return Err("not_configured");
    }
    let address = Address::from_str(&address).map_err(|_| "not_configured")?;
    let rpc_url: String = sqlx::query_scalar("SELECT rpc_url FROM evm_networks WHERE chain_id=?1")
        .bind(asset.chain_id)
        .fetch_one(db)
        .await
        .map_err(|_| "rpc_unavailable")?;
    let provider = rpc_provider(&rpc_url).map_err(|_| "rpc_unavailable")?;
    let block = provider
        .get_block_number()
        .await
        .map_err(|_| "rpc_unavailable")?;
    let contract = Erc20::new(
        Address::from_str(asset.contract_address).expect("built-in contract"),
        Arc::new(provider.clone()),
    );
    let call = contract.balance_of(address).block(block);
    let (token, native, price) = tokio::try_join!(
        async { call.call().await.map_err(|_| "rpc_unavailable") },
        async {
            provider
                .get_balance(address, Some(block.into()))
                .await
                .map_err(|_| "rpc_unavailable")
        },
        async {
            provider
                .get_gas_price()
                .await
                .map_err(|_| "rpc_unavailable")
        },
    )?;
    let fee = price
        .checked_mul(U256::from(TRANSFER_GAS_BUDGET * 2))
        .filter(|fee| !fee.is_zero())
        .ok_or("rpc_unavailable")?;
    Ok(Observation {
        token,
        native,
        fee,
        observed_at: Utc::now().to_rfc3339(),
    })
}

async fn reservations(
    db: &SqlitePool,
    asset: BuiltinAsset,
) -> Result<(U256, U256, bool), ApiError> {
    let rows = sqlx::query("SELECT w.asset_id,w.amount_usd_nanos,w.gas_reservation_wei FROM withdrawals w JOIN supported_assets a ON a.id=w.asset_id JOIN ledger_entries e ON e.id=w.ledger_entry_id WHERE a.chain_id=?1 AND (w.status IN ('awaiting_signer','submitted') OR e.status='pending')")
        .bind(asset.chain_id).fetch_all(db).await.map_err(db_error)?;
    let mut token = U256::zero();
    let mut gas = U256::zero();
    let mut unknown_gas = false;
    for row in rows {
        if row.get::<String, _>(0) == asset.id {
            token =
                token.saturating_add(usd_nanos_to_token_units(row.get(1), asset.token_decimals)?);
        }
        match row.get::<Option<String>, _>(2) {
            Some(value) => {
                gas = gas.saturating_add(
                    U256::from_dec_str(&value)
                        .map_err(|_| ApiError::conflict("liquidity_unavailable"))?,
                )
            }
            // Existing pre-migration requests drain normally, but cannot be budgeted safely.
            None => unknown_gas = true,
        }
    }
    Ok((token, gas, unknown_gas))
}

async fn availability(
    db: &SqlitePool,
    asset: BuiltinAsset,
    observation: Result<Observation, &'static str>,
) -> Result<Availability, ApiError> {
    let mut result = Availability {
        asset_id: asset.id.to_string(),
        chain_id: asset.chain_id,
        network_name: builtin_network(asset.chain_id)
            .expect("built-in network")
            .name
            .to_string(),
        asset_symbol: asset.symbol.to_string(),
        available_amount: "0".into(),
        available_usd_nanos: "0".into(),
        status: "unavailable",
        gas_sufficient: None,
        observed_at: None,
        reason: None,
    };
    let observation = match observation {
        Ok(value) => value,
        Err(reason) => {
            result.reason = Some(reason);
            return Ok(result);
        }
    };
    result.observed_at = Some(observation.observed_at.clone());
    let (token, gas, unknown) = reservations(db, asset).await?;
    let sufficient = !unknown && observation.native.saturating_sub(gas) >= observation.fee;
    result.gas_sufficient = Some(sufficient);
    let units = observation.token.saturating_sub(token);
    // Cap at the ledger's i64 range and round DOWN to representable nanodollars.
    let nanos = if asset.token_decimals > 9 {
        units / U256::exp10((asset.token_decimals - 9) as usize)
    } else {
        units.saturating_mul(U256::exp10((9 - asset.token_decimals) as usize))
    };
    let quantum = if asset.token_decimals < 9 {
        10_i64.pow((9 - asset.token_decimals) as u32)
    } else {
        1
    };
    let nanos = nanos.min(U256::from(i64::MAX as u64)).as_u64() as i64 / quantum * quantum;
    result.reason = if unknown {
        Some("pending_gas_unknown")
    } else if !sufficient {
        Some("insufficient_gas")
    } else if nanos == 0 {
        Some("insufficient_liquidity")
    } else {
        None
    };
    if result.reason.is_none() {
        result.status = "ready";
        result.available_usd_nanos = nanos.to_string();
        result.available_amount = format_usd(nanos);
    }
    Ok(result)
}

// Called while withdrawal_lock and write_lock are held, before any ledger write.
pub(crate) async fn validate(
    db: &SqlitePool,
    asset: BuiltinAsset,
    amount: i64,
    destination: &str,
) -> Result<U256, ApiError> {
    let observation = observe(db, asset).await.map_err(ApiError::conflict)?;
    let fee = observation.fee;
    let available = availability(db, asset, Ok(observation)).await?;
    if let Some(reason) = available.reason {
        return Err(ApiError::conflict(reason));
    }
    if amount
        > available
            .available_usd_nanos
            .parse::<i64>()
            .expect("exact nanodollars")
    {
        return Err(ApiError::conflict("insufficient_liquidity"));
    }
    time::timeout(
        RPC_TIMEOUT,
        validate_transfer_gas(db, asset, amount, destination, fee),
    )
    .await
    .map_err(|_| ApiError::conflict("rpc_unavailable"))??;
    Ok(fee)
}

async fn validate_transfer_gas(
    db: &SqlitePool,
    asset: BuiltinAsset,
    amount: i64,
    destination: &str,
    budget: U256,
) -> Result<(), ApiError> {
    let address = meta(db, CUSTODY_WALLET_ADDRESS_KEY)
        .await
        .map_err(db_error)?
        .ok_or_else(|| ApiError::conflict("not_configured"))?;
    let rpc_url: String = sqlx::query_scalar("SELECT rpc_url FROM evm_networks WHERE chain_id=?1")
        .bind(asset.chain_id)
        .fetch_one(db)
        .await
        .map_err(db_error)?;
    let provider = rpc_provider(&rpc_url).map_err(|_| ApiError::conflict("rpc_unavailable"))?;
    let call = Erc20::new(
        Address::from_str(asset.contract_address).expect("built-in contract"),
        Arc::new(provider.clone()),
    )
    .transfer(
        parse_address(destination, "destination_address")?,
        usd_nanos_to_token_units(amount, asset.token_decimals)?,
    )
    .from(parse_address(&address, "custody address")?);
    let gas = call
        .estimate_gas()
        .await
        .map_err(|_| ApiError::conflict("rpc_unavailable"))?;
    let price = provider
        .get_gas_price()
        .await
        .map_err(|_| ApiError::conflict("rpc_unavailable"))?;
    if gas.checked_mul(price).is_none_or(|cost| cost > budget) {
        return Err(ApiError::conflict("insufficient_gas"));
    }
    Ok(())
}

pub(crate) async fn protect_sweep_gas(
    db: &SqlitePool,
    chain_id: i64,
    native: U256,
    cost: U256,
) -> Result<(), ApiError> {
    let (_, gas, unknown) = reservations(db, builtin_asset_for_network(chain_id, "USDC")).await?;
    if unknown || native.saturating_sub(gas) < cost {
        return Err(ApiError::conflict("insufficient unreserved custody gas"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct Rpc {
        tokens: Arc<AtomicU64>,
        native: Arc<AtomicU64>,
        calls: Arc<AtomicU64>,
    }

    async fn fixture(
        tokens: u64,
        native: u64,
    ) -> (AppState, String, Rpc, tokio::task::JoinHandle<()>) {
        let rpc = Rpc {
            tokens: Arc::new(AtomicU64::new(tokens)),
            native: Arc::new(AtomicU64::new(native)),
            calls: Arc::new(AtomicU64::new(0)),
        };
        let handler = |State(rpc): State<Rpc>, Json(request): Json<serde_json::Value>| async move {
            rpc.calls.fetch_add(1, Ordering::SeqCst);
            let result = match request["method"].as_str().unwrap() {
                "eth_blockNumber" => serde_json::json!("0x123"),
                "eth_getBalance" => {
                    serde_json::json!(format!("0x{:x}", rpc.native.load(Ordering::SeqCst)))
                }
                "eth_gasPrice" => serde_json::json!("0x1"),
                "eth_estimateGas" => serde_json::json!("0x186a0"),
                "eth_call" => {
                    serde_json::json!(format!("0x{:064x}", rpc.tokens.load(Ordering::SeqCst)))
                }
                _ => {
                    return Json(
                        serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "error":{"code":-32000,"message":"mock signer offline with secret custody address"}}),
                    );
                }
            };
            Json(serde_json::json!({"jsonrpc":"2.0", "id":request["id"], "result":result}))
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let router = Router::new()
            .route("/", post(handler))
            .with_state(rpc.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        migrate(&db).await.unwrap();
        let user = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO users(id) VALUES(?1)")
            .bind(&user)
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ledger_entries(id,user_id,kind,status,amount_usd_nanos,balance_delta_usd_nanos,created_at) VALUES('funding',?1,'adjustment','posted',100000000000,100000000000,'now')").bind(&user).execute(&db).await.unwrap();
        let key = format!("{:064x}", 1);
        let address = format!("{:#x}", parse_wallet(&key, 1).unwrap().address());
        for (key, value) in [
            (ROOT_USER_ID_KEY, user.as_str()),
            (CUSTODY_WALLET_PRIVATE_KEY_KEY, &key),
            (CUSTODY_WALLET_ADDRESS_KEY, &address),
        ] {
            sqlx::query("INSERT OR REPLACE INTO app_meta(key,value) VALUES(?1,?2)")
                .bind(key)
                .bind(value)
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE evm_networks SET rpc_url=?1")
            .bind(&url)
            .execute(&db)
            .await
            .unwrap();
        let state = AppState {
            db,
            verifier: AuthMiniVerifier::from_issuer_background(
                &url,
                "test".into(),
                JwksCachePolicy::default(),
            )
            .unwrap(),
            write_lock: Arc::new(Mutex::new(())),
            withdrawal_lock: Arc::new(Mutex::new(())),
            sweep_lock: Arc::new(Mutex::new(())),
            liquidity_cache: Arc::new(Mutex::new(Cache::default())),
        };
        (state, user, rpc, server)
    }

    fn request(amount: i64) -> WithdrawalRequest {
        WithdrawalRequest {
            asset_id: "1-USDC".into(),
            destination_address: "0x0000000000000000000000000000000000000002".into(),
            amount_usd_nanos: amount,
            note: None,
        }
    }

    #[tokio::test]
    async fn rejected_liquidity_and_gas_never_debit_or_reserve() {
        for (tokens, native, reason) in [
            (500_000, 10_000_000, "insufficient_liquidity"),
            (2_000_000, 399_999, "insufficient_gas"),
        ] {
            let (state, user, _, server) = fixture(tokens, native).await;
            let error = post_withdrawal(&state, &user, &request(1_000_000_000), "reject")
                .await
                .err()
                .unwrap();
            assert_eq!(error.status, StatusCode::CONFLICT);
            assert_eq!(error.message, reason);
            assert_eq!(
                available_balance(&state.db, &user).await.unwrap(),
                100_000_000_000
            );
            for table in ["withdrawals", "payment_operations"] {
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                        .fetch_one(&state.db)
                        .await
                        .unwrap(),
                    0
                );
            }
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM ledger_entries")
                    .fetch_one(&state.db)
                    .await
                    .unwrap(),
                1
            );
            server.abort();
        }
    }

    #[tokio::test]
    async fn concurrent_creations_cannot_oversubscribe_and_replay_needs_no_rpc() {
        let (state, user, rpc, server) = fixture(1_000_000, 10_000_000).await;
        let input = request(750_000_000);
        let (a, b) = tokio::join!(
            post_withdrawal(&state, &user, &input, "a"),
            post_withdrawal(&state, &user, &input, "b")
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        let (accepted, key) = if let Ok(accepted) = a {
            (accepted, "a")
        } else {
            (b.unwrap(), "b")
        };
        assert_eq!(
            available_balance(&state.db, &user).await.unwrap(),
            99_250_000_000
        );
        rpc.tokens.store(0, Ordering::SeqCst);
        server.abort();
        let replay = post_withdrawal(&state, &user, &input, key).await.unwrap();
        assert_eq!(accepted.id, replay.id);
    }

    #[tokio::test]
    async fn fresh_creation_ignores_read_cache_and_failures_are_safe() {
        let (state, user, rpc, server) = fixture(2_000_000, 10_000_000).await;
        let asset = builtin_asset("1-USDC").unwrap();
        let first = cached_observation(&state.db, &state.liquidity_cache, asset)
            .await
            .unwrap();
        let calls = rpc.calls.load(Ordering::SeqCst);
        let again = cached_observation(&state.db, &state.liquidity_cache, asset)
            .await
            .unwrap();
        assert_eq!(rpc.calls.load(Ordering::SeqCst), calls);
        assert_eq!(first.observed_at, again.observed_at);
        rpc.tokens.store(0, Ordering::SeqCst);
        let error = post_withdrawal(&state, &user, &request(1_000_000_000), "fresh")
            .await
            .err()
            .unwrap();
        assert_eq!(error.message, "insufficient_liquidity");
        state
            .liquidity_cache
            .lock()
            .await
            .entries
            .get_mut(asset.id)
            .unwrap()
            .0 = Instant::now() - CACHE_TTL;
        assert_eq!(
            cached_observation(&state.db, &state.liquidity_cache, asset)
                .await
                .unwrap()
                .token,
            U256::zero()
        );
        server.abort();
        state.liquidity_cache.lock().await.invalidate();
        let result = cached_observation(&state.db, &state.liquidity_cache, asset).await;
        let response = availability(&state.db, asset, result).await.unwrap();
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(response.reason, Some("rpc_unavailable"));
        assert!(!json.contains("0x"));
        assert!(!json.contains("http"));
        let error = post_withdrawal(&state, &user, &request(1_000_000_000), "offline")
            .await
            .err()
            .unwrap();
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(
            available_balance(&state.db, &user).await.unwrap(),
            100_000_000_000
        );
    }

    #[tokio::test]
    async fn submitted_reservations_and_cross_asset_gas_are_counted_until_settled() {
        let (state, user, _, server) = fixture(2_000_000, 700_000).await;
        let withdrawal = post_withdrawal(&state, &user, &request(1_000_000_000), "one")
            .await
            .unwrap();
        let _execution = state.withdrawal_lock.lock().await;
        sqlx::query("UPDATE withdrawals SET status='submitted',signed_transaction='0x01',transaction_hash='hash' WHERE id=?1").bind(&withdrawal.id).execute(&state.db).await.unwrap();
        let asset = builtin_asset("1-USDC").unwrap();
        let (token, gas, unknown) = reservations(&state.db, asset).await.unwrap();
        assert_eq!(token, U256::from(1_000_000));
        assert_eq!(gas, U256::from(400_000));
        assert!(!unknown);
        let usdt = builtin_asset("1-USDT").unwrap();
        let response = availability(&state.db, usdt, observe(&state.db, usdt).await)
            .await
            .unwrap();
        assert_eq!(response.reason, Some("insufficient_gas"));
        assert!(
            protect_sweep_gas(&state.db, 1, U256::from(700_000), U256::from(400_000))
                .await
                .is_err()
        );
        cached_observation(&state.db, &state.liquidity_cache, asset)
            .await
            .unwrap();
        settle_withdrawal(
            &state.db,
            &state.write_lock,
            &state.liquidity_cache,
            &withdrawal.id,
            false,
        )
        .await
        .unwrap();
        assert!(state.liquidity_cache.lock().await.entries.is_empty());
        assert_eq!(
            reservations(&state.db, asset).await.unwrap(),
            (U256::zero(), U256::zero(), false)
        );
        assert_eq!(
            available_balance(&state.db, &user).await.unwrap(),
            100_000_000_000
        );
        server.abort();
    }

    #[tokio::test]
    async fn availability_precision_and_authorization() {
        let (state, _, _, server) = fixture(0, 1_000_000).await;
        let asset = builtin_asset("56-USDT").unwrap();
        let observation = Observation {
            token: U256::from(1_234_567_890_123_456_789_u64),
            native: U256::from(1_000_000),
            fee: U256::from(400_000),
            observed_at: "now".into(),
        };
        let available = availability(&state.db, asset, Ok(observation))
            .await
            .unwrap();
        assert_eq!(available.available_usd_nanos, "1234567890");
        assert_eq!(available.available_amount, "1.234567890");
        for key in [None, Some("fund-api-key")] {
            let mut request =
                axum::http::Request::builder().uri("/api/withdrawal-availability?asset_id=1-USDC");
            if let Some(key) = key {
                request = request.header("x-api-key", key);
            }
            let response = app(state.clone())
                .oneshot(request.body(axum::body::Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        server.abort();
    }
}
