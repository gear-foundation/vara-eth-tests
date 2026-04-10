use alloy::{primitives::U256 as AlloyU256, providers::Provider, sol_types::SolEvent};
use anyhow::{Context, Result, anyhow, ensure};
use ethexe_common::gear::CodeState;
use ethexe_ethereum::{TryGetReceipt, abi::{IMirror, IRouter, IWrappedVara}};
use gear_core::ids::prelude::CodeIdExt as _;
use parity_scale_codec::{Decode, Encode};
use rust_sdk_tests::{
    config::TestConfig,
    test_helpers::{
        MAX_POLL_ATTEMPTS, POLL_DELAY, assert_auto_success, assert_manual_success,
        extract_message_id_from_receipt, init_tracing, log_step, top_up_executable_balance,
        wait_for_executable_balance, wait_for_program_on_vara_eth, wait_for_state_hash_change,
    },
};
use std::{fs, path::Path, str::FromStr};
use tokio::time::{Duration, sleep, timeout};
use tracing::info;

use ethexe_sdk::VaraEthApi;
use gprimitives::{ActorId, CodeId, H256};

const CHECKER_WASM_PATH: &str = "../target/wasm32-gear/release/mandelbrot_checker.opt.wasm";
const MANAGER_WASM_PATH: &str = "../target/wasm32-gear/release/manager.opt.wasm";
const CHECKER_COUNT: usize = 3;
const CHECKER_TOP_UP_AMOUNT: u128 = 2000 * 1_000_000_000_000;
const MANAGER_TOP_UP_AMOUNT: u128 = 15000 * 1_000_000_000_000;
const VARA_DECIMALS: u128 = 1_000_000_000_000;
const RAW_TX_GAS_PRICE_BUMP_NUMERATOR: u128 = 6;
const RAW_TX_GAS_PRICE_BUMP_DENOMINATOR: u128 = 5;
const APPROVE_SUBMIT_TIMEOUT: Duration = Duration::from_secs(45);
const APPROVE_SUBMIT_MAX_ATTEMPTS: usize = 3;
const CODE_VALIDATION_TIMEOUT: Duration = Duration::from_secs(180);

struct StressScenario {
    name: &'static str,
    width: u32,
    height: u32,
    points_per_call: u32,
    max_iter: u32,
    batch_size: u32,
    x_min_num: i64,
    x_min_scale: u32,
    x_max_num: i64,
    x_max_scale: u32,
    y_min_num: i64,
    y_min_scale: u32,
    y_max_num: i64,
    y_max_scale: u32,
}

const STRESS_SCENARIOS: &[StressScenario] = &[
    StressScenario {
        name: "grid_200x200",
        width: 200,
        height: 200,
        points_per_call: 30_000,
        max_iter: 1_000,
        batch_size: 10,
        x_min_num: -2,
        x_min_scale: 0,
        x_max_num: 1,
        x_max_scale: 0,
        y_min_num: -15,
        y_min_scale: 1,
        y_max_num: 15,
        y_max_scale: 1,
    },
    StressScenario {
        name: "grid_400x400",
        width: 400,
        height: 400,
        points_per_call: 30_000,
        max_iter: 1_000,
        batch_size: 10,
        x_min_num: -2,
        x_min_scale: 0,
        x_max_num: 1,
        x_max_scale: 0,
        y_min_num: -15,
        y_min_scale: 1,
        y_max_num: 15,
        y_max_scale: 1,
    },
];

#[allow(dead_code)]
const PREDEPLOYED_CHECKER_IDS: &[&str] = &[
    "0x00000000000000000000000073e1f82abe65f9eaeef700947f7f4ddb723bcbed",
    "0x000000000000000000000000c5d2b13f8927a441437f5ca17441979d8fd81150",
    "0x0000000000000000000000000ba1d512fa9e652851e2be2cd82c18a8d6040b15",
];

struct TestContext {
    api: VaraEthApi,
    checker_ids: Vec<ActorId>,
    manager_id: ActorId,
}

fn predeployed_checker_ids() -> Result<Vec<ActorId>> {
    ensure!(
        PREDEPLOYED_CHECKER_IDS.len() == CHECKER_COUNT,
        "expected {CHECKER_COUNT} predeployed checker ids, got {}",
        PREDEPLOYED_CHECKER_IDS.len()
    );

    PREDEPLOYED_CHECKER_IDS
        .iter()
        .enumerate()
        .map(|(index, checker_id)| {
            ActorId::from_str(checker_id).with_context(|| {
                format!("failed to parse PREDEPLOYED_CHECKER_IDS[{index}] = {checker_id}")
            })
        })
        .collect()
}

fn format_vara(amount: u128) -> String {
    let whole = amount / VARA_DECIMALS;
    let frac = amount % VARA_DECIMALS;
    format!("{whole}.{frac:012}")
}

fn init_payload() -> Vec<u8> {
    ["Init".to_string().encode(), ().encode()].concat()
}

fn manager_payload<T: Encode>(action: &str, args: T) -> Vec<u8> {
    ["Manager".to_string().encode(), action.to_string().encode(), args.encode()].concat()
}

fn extract_program_id_from_receipt(
    receipt: &alloy::rpc::types::TransactionReceipt,
) -> Result<ActorId> {
    receipt
        .inner
        .logs()
        .iter()
        .find_map(|log| IRouter::ProgramCreated::decode_log(log.as_ref()).ok())
        .map(|event| (*event.actorId.into_word()).into())
        .ok_or_else(|| anyhow!("couldn't find `ProgramCreated` log in checker deployment receipt"))
}

fn effective_code_id(configured_code_id: CodeId, wasm_path: &str, label: &str) -> Result<CodeId> {
    let code =
        fs::read(Path::new(wasm_path)).with_context(|| format!("failed to read {wasm_path}"))?;
    let local_code_id = CodeId::generate(&code);

    if configured_code_id != local_code_id {
        info!(
            label,
            configured_code_id = %configured_code_id,
            local_code_id = %local_code_id,
            "Configured code id differs from local wasm; using local code id"
        );
    }

    Ok(local_code_id)
}

async fn bumped_gas_price(
    provider: &impl Provider,
) -> Result<u128> {
    let gas_price = provider
        .get_gas_price()
        .await
        .with_context(|| "failed to query current gas price for raw checker txs")?;

    Ok(gas_price
        .saturating_mul(RAW_TX_GAS_PRICE_BUMP_NUMERATOR)
        / RAW_TX_GAS_PRICE_BUMP_DENOMINATOR)
}

fn is_retryable_approve_submit_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("replacement transaction underpriced")
        || message.contains("underpriced")
        || message.contains("timeout")
        || message.contains("timed out")
}

fn is_restart_required_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("restart required")
        || message.contains("connection closed")
        || message.contains("background task closed")
}

async fn connect_fresh_api() -> Result<VaraEthApi> {
    let config = TestConfig::load()?;
    config.connect_api().await
}

async fn calculate_manager_reply_once(
    api: &VaraEthApi,
    manager_id: ActorId,
    payload: Vec<u8>,
) -> Result<gear_core::rpc::ReplyInfo> {
    api.mirror(manager_id)
        .calculate_reply_for_handle(payload, 0)
        .await
        .with_context(|| "failed to calculate manager reply")
}

async fn executable_balance(api: &VaraEthApi, program_id: ActorId) -> Result<u128> {
    match api.mirror(program_id).state().await {
        Ok(state) => Ok(state.executable_balance),
        Err(error) => {
            let error = anyhow!(error).context(format!(
                "failed to read executable balance for program {program_id}"
            ));
            if !is_restart_required_error(&error) {
                return Err(error);
            }

            info!(
                %program_id,
                error = %error,
                "Vara.Eth API connection dropped while reading executable balance; reconnecting"
            );
            let fresh_api = connect_fresh_api().await?;
            Ok(fresh_api.mirror(program_id).state().await?.executable_balance)
        }
    }
}

async fn sender_nonces(
    provider: &impl Provider,
    sender_address: alloy::primitives::Address,
) -> Result<(u64, u64)> {
    let latest_nonce = provider
        .get_transaction_count(sender_address)
        .latest()
        .await
        .with_context(|| "failed to query latest sender nonce")?;
    let pending_nonce = provider
        .get_transaction_count(sender_address)
        .pending()
        .await
        .with_context(|| "failed to query pending sender nonce")?;

    Ok((latest_nonce, pending_nonce))
}

async fn submit_checker_approve_with_retry<P>(
    provider: P,
    wvara_address: alloy::primitives::Address,
    sender_address: alloy::primitives::Address,
    checker_id: ActorId,
    checker_index: usize,
    approve_nonce: u64,
    initial_gas_price: u128,
) -> Result<impl TryGetReceipt<alloy::network::Ethereum>>
where
    P: Provider + Clone,
{
    let mirror_address: alloy::primitives::Address =
        checker_id.to_address_lossy().to_fixed_bytes().into();
    let mut gas_price = initial_gas_price;

    for attempt in 1..=APPROVE_SUBMIT_MAX_ATTEMPTS {
        let (latest_nonce, pending_nonce) = sender_nonces(&provider, sender_address).await?;
        info!(
            checker_index,
            %checker_id,
            attempt,
            latest_nonce,
            pending_nonce,
            approve_nonce,
            gas_price,
            "Submitting checker approve transaction"
        );

        let submit_result = timeout(
            APPROVE_SUBMIT_TIMEOUT,
            IWrappedVara::IWrappedVaraInstance::new(wvara_address, provider.clone())
                .approve(mirror_address, AlloyU256::from(CHECKER_TOP_UP_AMOUNT))
                .gas_price(gas_price)
                .nonce(approve_nonce)
                .send(),
        )
        .await;

        match submit_result {
            Ok(Ok(pending_tx)) => return Ok(pending_tx),
            Ok(Err(error)) => {
                let error = anyhow::Error::from(error).context(format!(
                    "checker approve submit failed for checker-{checker_index}"
                ));
                if attempt == APPROVE_SUBMIT_MAX_ATTEMPTS || !is_retryable_approve_submit_error(&error)
                {
                    return Err(error);
                }
                let refreshed_gas_price = bumped_gas_price(&provider).await?;
                gas_price = gas_price
                    .max(refreshed_gas_price)
                    .saturating_mul(RAW_TX_GAS_PRICE_BUMP_NUMERATOR)
                    / RAW_TX_GAS_PRICE_BUMP_DENOMINATOR;
                info!(
                    checker_index,
                    %checker_id,
                    attempt,
                    refreshed_gas_price,
                    next_gas_price = gas_price,
                    error = %error,
                    "Retrying checker approve after temporary submit error"
                );
            }
            Err(_) => {
                let error = anyhow!(
                    "timed out while submitting checker approve for checker-{checker_index}"
                );
                if attempt == APPROVE_SUBMIT_MAX_ATTEMPTS || !is_retryable_approve_submit_error(&error)
                {
                    return Err(error);
                }
                let refreshed_gas_price = bumped_gas_price(&provider).await?;
                gas_price = gas_price
                    .max(refreshed_gas_price)
                    .saturating_mul(RAW_TX_GAS_PRICE_BUMP_NUMERATOR)
                    / RAW_TX_GAS_PRICE_BUMP_DENOMINATOR;
                info!(
                    checker_index,
                    %checker_id,
                    attempt,
                    refreshed_gas_price,
                    next_gas_price = gas_price,
                    "Retrying checker approve after submit timeout"
                );
            }
        }
    }

    unreachable!("approve submit attempts should always return or error");
}

async fn query_manager<T: Decode>(
    api: &VaraEthApi,
    manager_id: ActorId,
    action: &str,
    payload: Vec<u8>,
) -> Result<T> {
    let reply = match calculate_manager_reply_once(api, manager_id, payload.clone()).await {
        Ok(reply) => reply,
        Err(error) => {
            let error = error.context(format!(
                "failed to calculate manager reply for action {action}"
            ));
            if !is_restart_required_error(&error) {
                return Err(error);
            }

            info!(
                action,
                error = %error,
                "Vara.Eth API connection dropped during manager query; reconnecting"
            );

            let fresh_api = connect_fresh_api().await?;
            calculate_manager_reply_once(&fresh_api, manager_id, payload)
                .await
                .with_context(|| {
                    format!("failed to calculate manager reply for action {action} after reconnect")
                })?
        }
    };

    assert_manual_success(&reply.code.to_bytes(), action);

    let (service, returned_action, value) =
        <(String, String, T)>::decode(&mut reply.payload.as_ref())
            .with_context(|| format!("failed to decode manager reply envelope for {action}"))?;

    assert_eq!(service, "Manager");
    assert_eq!(returned_action, action);

    Ok(value)
}

async fn manager_points_sent(api: &VaraEthApi, manager_id: ActorId) -> Result<u32> {
    query_manager(api, manager_id, "PointsSent", manager_payload("PointsSent", ())).await
}

async fn manager_checked_count(api: &VaraEthApi, manager_id: ActorId) -> Result<u32> {
    query_manager(
        api,
        manager_id,
        "GetCheckedCount",
        manager_payload("GetCheckedCount", ()),
    )
    .await
}

async fn manager_checkers(api: &VaraEthApi, manager_id: ActorId) -> Result<Vec<ActorId>> {
    let encoded: Vec<u8> =
        query_manager(api, manager_id, "GetCheckers", manager_payload("GetCheckers", ())).await?;
    Vec::<ActorId>::decode(&mut encoded.as_slice())
        .with_context(|| "failed to decode manager checker list")
}

async fn wait_for_programs_on_vara_eth(api: &VaraEthApi, program_ids: &[ActorId]) -> Result<()> {
    for _ in 0..MAX_POLL_ATTEMPTS {
        let ids = api.router().program_ids().await?;
        let visible_programs = program_ids
            .iter()
            .filter(|program_id| ids.contains(program_id))
            .count();
        info!(
            expected_programs = program_ids.len(),
            visible_programs,
            "Waiting for checker programs to appear on Vara.Eth"
        );
        if program_ids.iter().all(|program_id| ids.contains(program_id)) {
            return Ok(());
        }
        sleep(POLL_DELAY).await;
    }

    anyhow::bail!("programs did not appear on Vara.Eth within bounded polling window");
}

async fn top_up_and_init_program(
    api: &VaraEthApi,
    program_id: ActorId,
    top_up_amount: u128,
    label: &str,
) -> Result<()> {
    top_up_executable_balance(api, program_id, top_up_amount)
        .await
        .with_context(|| format!("failed to top up executable balance for {label}"))?;

    let mirror = api.mirror(program_id);
    let previous_state_hash = mirror.state_hash().await?;
    let (receipt, message_id) = mirror
        .send_message_with_receipt(init_payload(), 0)
        .await
        .with_context(|| format!("failed to send init message for {label}"))?;

    assert!(receipt.status(), "expected successful init receipt for {label}");

    let reply = mirror
        .wait_for_reply(message_id)
        .await
        .with_context(|| format!("failed to wait for init reply for {label}"))?;
    assert_auto_success(&reply.code.to_bytes(), label);

    let state_hash =
        wait_for_state_hash_change(api, program_id, previous_state_hash).await?;
    info!(label, %program_id, %state_hash, "Program initialized");
    Ok(())
}

async fn top_up_and_init_checker_programs(
    api: &VaraEthApi,
    ethereum: &ethexe_ethereum::Ethereum,
    checker_ids: &[ActorId],
) -> Result<()> {
    let provider = ethereum.provider();
    let sender_address = ethereum
        .sender_address()
        .ok_or_else(|| anyhow!("no sender address available for checker setup"))?;
    let base_account_nonce = provider.get_transaction_count(sender_address.into()).pending().await?;
    let (latest_nonce, pending_nonce) = sender_nonces(&provider, sender_address.into()).await?;
    let gas_price = bumped_gas_price(&provider).await?;
    let wvara_address: alloy::primitives::Address = ethereum.wrapped_vara().address().into();
    let init_payload = init_payload();
    info!(
        checker_count = checker_ids.len(),
        latest_nonce,
        pending_nonce,
        base_account_nonce,
        gas_price,
        top_up_amount = CHECKER_TOP_UP_AMOUNT,
        "Starting raw checker top-up and init setup"
    );
    let previous_state_hashes = futures_util::future::try_join_all(
        checker_ids
            .iter()
            .copied()
            .map(|checker_id| async move { api.mirror(checker_id).state_hash().await }),
    )
    .await?;
    info!(
        checker_count = previous_state_hashes.len(),
        "Captured previous checker state hashes before raw setup"
    );

    let mut approve_pending = Vec::with_capacity(checker_ids.len());
    for (index, checker_id) in checker_ids.iter().copied().enumerate() {
        let nonce = base_account_nonce + index as u64;
        let pending_tx = submit_checker_approve_with_retry(
            provider.clone(),
            wvara_address,
            sender_address.into(),
            checker_id,
            index,
            nonce,
            gas_price,
        )
        .await?;
        approve_pending.push(pending_tx);
    }
    let approve_receipts = futures_util::future::try_join_all(
        approve_pending
            .into_iter()
            .map(|pending_tx| pending_tx.try_get_receipt_check_reverted()),
    )
    .await?;
    for (index, receipt) in approve_receipts.iter().enumerate() {
        assert!(receipt.status(), "expected successful checker approve receipt");
        info!(
            checker_index = index,
            checker_id = %checker_ids[index],
            tx_hash = %receipt.transaction_hash,
            "Checker approve completed"
        );
    }

    let (top_up_latest_nonce, top_up_pending_nonce) =
        sender_nonces(&provider, sender_address.into()).await?;
    let top_up_base_nonce = top_up_pending_nonce;
    info!(
        top_up_latest_nonce,
        top_up_pending_nonce,
        top_up_base_nonce,
        "Preparing checker top-up phase"
    );

    let mut top_up_pending = Vec::with_capacity(checker_ids.len());
    for (index, checker_id) in checker_ids.iter().copied().enumerate() {
        let nonce = top_up_base_nonce + index as u64;
        let mirror_address: alloy::primitives::Address =
            checker_id.to_address_lossy().to_fixed_bytes().into();
        info!(
            checker_index = index,
            %checker_id,
            nonce,
            "Submitting checker executable balance top-up transaction"
        );
        let pending_tx = IMirror::IMirrorInstance::new(mirror_address, provider.clone())
            .executableBalanceTopUp(CHECKER_TOP_UP_AMOUNT)
            .gas_price(gas_price)
            .nonce(nonce)
            .send()
            .await
            .with_context(|| format!("failed to submit checker top-up for checker-{index}"))?;
        top_up_pending.push(pending_tx);
    }

    let top_up_receipts = futures_util::future::try_join_all(
        top_up_pending
            .into_iter()
            .map(|pending_tx| pending_tx.try_get_receipt_check_reverted()),
    )
    .await?;
    for (index, receipt) in top_up_receipts.iter().enumerate() {
        assert!(receipt.status(), "expected successful checker top-up receipt");
        info!(
            checker_index = index,
            checker_id = %checker_ids[index],
            tx_hash = %receipt.transaction_hash,
            "Checker executable balance top-up completed"
        );
    }
    futures_util::future::try_join_all(
        checker_ids
            .iter()
            .copied()
            .map(|checker_id| async move {
                wait_for_executable_balance(api, checker_id, CHECKER_TOP_UP_AMOUNT).await
            }),
    )
    .await?;
    let checker_balances = futures_util::future::try_join_all(
        checker_ids
            .iter()
            .copied()
            .map(|checker_id| async move { Ok::<u128, anyhow::Error>(api.mirror(checker_id).state().await?.executable_balance) }),
    )
    .await?;
    for (index, executable_balance) in checker_balances.iter().enumerate() {
        info!(
            checker_index = index,
            checker_id = %checker_ids[index],
            executable_balance,
            executable_balance_vara = %format_vara(*executable_balance),
            "Checker executable balance observed after top-up"
        );
    }

    let (init_latest_nonce, init_pending_nonce) = sender_nonces(&provider, sender_address.into()).await?;
    let init_base_nonce = init_pending_nonce;
    info!(
        init_latest_nonce,
        init_pending_nonce,
        init_base_nonce,
        "Preparing checker init phase"
    );

    let mut init_pending = Vec::with_capacity(checker_ids.len());
    for (index, checker_id) in checker_ids.iter().copied().enumerate() {
        let nonce = init_base_nonce + index as u64;
        let mirror_address: alloy::primitives::Address =
            checker_id.to_address_lossy().to_fixed_bytes().into();
        info!(
            checker_index = index,
            %checker_id,
            nonce,
            "Submitting checker init transaction"
        );
        let pending_tx = IMirror::IMirrorInstance::new(mirror_address, provider.clone())
            .sendMessage(init_payload.clone().into(), false)
            .gas_price(gas_price)
            .value(AlloyU256::from(0))
            .nonce(nonce)
            .send()
            .await
            .with_context(|| format!("failed to submit checker init for checker-{index}"))?;
        init_pending.push(pending_tx);
    }

    let init_receipts = futures_util::future::try_join_all(
        init_pending
            .into_iter()
            .map(|pending_tx| pending_tx.try_get_receipt_check_reverted()),
    )
    .await?;
    for (index, receipt) in init_receipts.iter().enumerate() {
        info!(
            checker_index = index,
            checker_id = %checker_ids[index],
            tx_hash = %receipt.transaction_hash,
            "Checker init transaction completed"
        );
    }
    let init_message_ids = init_receipts
        .iter()
        .map(extract_message_id_from_receipt)
        .collect::<Result<Vec<_>>>()?;

    let init_replies = futures_util::future::try_join_all(
        checker_ids
            .iter()
            .copied()
            .zip(init_message_ids.iter().copied())
            .map(|(checker_id, message_id)| async move { api.mirror(checker_id).wait_for_reply(message_id).await }),
    )
    .await?;
    for (index, reply) in init_replies.iter().enumerate() {
        assert_auto_success(&reply.code.to_bytes(), &format!("checker-{index}"));
        info!(
            checker_index = index,
            checker_id = %checker_ids[index],
            message_id = %init_message_ids[index],
            "Checker init reply received"
        );
    }

    let state_hashes = futures_util::future::try_join_all(
        checker_ids
            .iter()
            .copied()
            .zip(previous_state_hashes.into_iter())
            .map(|(checker_id, previous_state_hash)| async move {
                wait_for_state_hash_change(api, checker_id, previous_state_hash).await
            }),
    )
    .await?;
    for (index, state_hash) in state_hashes.iter().enumerate() {
        info!(
            checker_index = index,
            checker_id = %checker_ids[index],
            %state_hash,
            "Checker state hash changed after init"
        );
    }

    Ok(())
}

async fn ensure_manager_code_validated(
    api: &VaraEthApi,
    manager_code_id: CodeId,
) -> Result<()> {
    let router = api.router();
    let code_state = router
        .code_state(manager_code_id)
        .await
        .with_context(|| "failed to query manager code state")?;

    info!(?code_state, manager_code_id = %manager_code_id, "Manager code state");
    if matches!(code_state, CodeState::Validated) {
        return Ok(());
    }

    let code = fs::read(Path::new(MANAGER_WASM_PATH))
        .with_context(|| format!("failed to read {MANAGER_WASM_PATH}"))?;

    info!("Requesting manager code validation");
    let (_receipt, requested_code_id) = router
        .request_code_validation_with_receipt(&code)
        .await
        .with_context(|| "failed to request manager code validation")?;
    ensure!(
        requested_code_id == manager_code_id,
        "requested manager code id does not match expected MANAGER_CODE_ID"
    );

    let validation = timeout(
        CODE_VALIDATION_TIMEOUT,
        router.wait_for_code_validation(manager_code_id),
    )
    .await
    .with_context(|| "timed out while waiting for manager code validation")??;
    ensure!(validation.valid, "manager code validation returned invalid");
    Ok(())
}

async fn ensure_checker_code_validated(
    api: &VaraEthApi,
    checker_code_id: CodeId,
) -> Result<()> {
    let router = api.router();
    let code_state = router
        .code_state(checker_code_id)
        .await
        .with_context(|| "failed to query checker code state")?;

    info!(?code_state, checker_code_id = %checker_code_id, "Checker code state");
    if matches!(code_state, CodeState::Validated) {
        return Ok(());
    }

    let code = fs::read(Path::new(CHECKER_WASM_PATH))
        .with_context(|| format!("failed to read {CHECKER_WASM_PATH}"))?;

    info!("Requesting checker code validation");
    let (_receipt, requested_code_id) = router
        .request_code_validation_with_receipt(&code)
        .await
        .with_context(|| "failed to request checker code validation")?;
    ensure!(
        requested_code_id == checker_code_id,
        "requested checker code id does not match expected CHECKER_CODE_ID"
    );

    let validation = timeout(
        CODE_VALIDATION_TIMEOUT,
        router.wait_for_code_validation(checker_code_id),
    )
    .await
    .with_context(|| "timed out while waiting for checker code validation")??;
    ensure!(validation.valid, "checker code validation returned invalid");
    Ok(())
}

async fn deploy_and_prepare_checkers(
    raw_api: &VaraEthApi,
    raw_ethereum: &ethexe_ethereum::Ethereum,
    config: &TestConfig,
) -> Result<Vec<ActorId>> {
    let checker_code_id = effective_code_id(config.checker_code_id()?, CHECKER_WASM_PATH, "checker")?;
    log_step("Deploy Checkers");
    ensure_checker_code_validated(&raw_api, checker_code_id).await?;
    let provider = raw_ethereum.provider();
    let sender_address = raw_ethereum
        .sender_address()
        .ok_or_else(|| anyhow!("no sender address available for checker deployment"))?;
    let base_account_nonce = provider.get_transaction_count(sender_address.into()).pending().await?;
    let gas_price = bumped_gas_price(&provider).await?;
    let router_address = config.router_address()?;

    let pending_txs = futures_util::future::try_join_all((0..CHECKER_COUNT).map(|index| {
        let provider = provider.clone();
        let salt = H256::random();
        async move {
            IRouter::IRouterInstance::new(router_address.into(), provider)
                .createProgram(
                    checker_code_id.into_bytes().into(),
                    salt.to_fixed_bytes().into(),
                    Default::default(),
                )
                .gas_price(gas_price)
                .nonce(base_account_nonce + index as u64)
                .send()
                .await
                .map_err(anyhow::Error::from)
        }
    }))
    .await
    .with_context(|| "failed to deploy checker programs")?;

    let receipts = futures_util::future::try_join_all(
        pending_txs
            .into_iter()
            .map(|pending_tx| pending_tx.try_get_receipt_check_reverted()),
    )
    .await
    .with_context(|| "failed to wait for checker deployment receipts")?;

    let checker_ids: Vec<ActorId> = receipts
        .iter()
        .map(|receipt| {
            assert!(receipt.status(), "expected successful checker deployment receipt");
            extract_program_id_from_receipt(receipt)
        })
        .collect::<Result<Vec<_>>>()?;
    for (index, checker_id) in checker_ids.iter().enumerate() {
        info!(
            checker_index = index,
            %checker_id,
            "Checker deployed"
        );
    }

    wait_for_programs_on_vara_eth(&raw_api, &checker_ids).await?;
    info!(checker_count = checker_ids.len(), "All checker programs appeared on Vara.Eth");
    top_up_and_init_checker_programs(&raw_api, &raw_ethereum, &checker_ids).await?;

    Ok(checker_ids)
}

async fn setup_manager_and_checkers() -> Result<TestContext> {
    let config = TestConfig::load()?;
    let raw_ethereum = config.connect_ethereum().await?;
    let raw_api = VaraEthApi::new(&config.vara_eth_rpc, raw_ethereum.clone())
        .await
        .with_context(|| "failed to connect Vara.Eth API client")?;
    let checker_ids = deploy_and_prepare_checkers(&raw_api, &raw_ethereum, &config).await?;

    setup_manager_with_checker_ids(&config, checker_ids).await
}

async fn setup_manager_with_checker_ids(
    config: &TestConfig,
    checker_ids: Vec<ActorId>,
) -> Result<TestContext> {
    let manager_code_id = effective_code_id(config.manager_code_id()?, MANAGER_WASM_PATH, "manager")?;

    // Recreate clients after raw nonce-managed phases so the SDK/provider nonce
    // manager is resynchronized from the chain before manager-side txs.
    let ethereum = config.connect_ethereum().await?;
    let api = VaraEthApi::new(&config.vara_eth_rpc, ethereum)
        .await
        .with_context(|| "failed to reconnect Vara.Eth API client after raw checker setup")?;
    let router = api.router();

    log_step("Deploy Manager");
    ensure_manager_code_validated(&api, manager_code_id).await?;
    let (manager_receipt, manager_id) = router
        .create_program_with_receipt(manager_code_id, H256::random(), None)
        .await
        .with_context(|| "failed to deploy manager program")?;
    assert!(
        manager_receipt.status(),
        "expected successful manager deployment receipt"
    );
    wait_for_program_on_vara_eth(&api, manager_id).await?;
    top_up_and_init_program(&api, manager_id, MANAGER_TOP_UP_AMOUNT, "manager").await?;

    log_step("Register Checkers");
    let manager_mirror = api.mirror(manager_id);
    for (index, checker_id) in checker_ids.iter().copied().enumerate() {
        let previous_state_hash = manager_mirror.state_hash().await?;
        let (receipt, message_id) = manager_mirror
            .send_message_with_receipt(manager_payload("AddChecker", (checker_id,)), 0)
            .await
            .with_context(|| format!("failed to send AddChecker for checker-{index}"))?;
        assert!(receipt.status(), "expected successful AddChecker receipt");
        let reply = manager_mirror
            .wait_for_reply(message_id)
            .await
            .with_context(|| format!("failed to wait for AddChecker reply for checker-{index}"))?;
        assert_auto_success(&reply.code.to_bytes(), &format!("manager add_checker-{index}"));
        wait_for_state_hash_change(&api, manager_id, previous_state_hash).await?;
        info!(
            checker_index = index,
            %checker_id,
            "Checker registered in manager"
        );
    }

    let registered_checkers = manager_checkers(&api, manager_id).await?;
    assert_eq!(registered_checkers.len(), CHECKER_COUNT);
    assert_eq!(registered_checkers, checker_ids);

    Ok(TestContext {
        api,
        checker_ids,
        manager_id,
    })
}

async fn setup_manager_with_predeployed_checkers() -> Result<TestContext> {
    let config = TestConfig::load()?;
    let checker_ids = predeployed_checker_ids()?;
    let ethereum = config.connect_ethereum().await?;
    let api = VaraEthApi::new(&config.vara_eth_rpc, ethereum)
        .await
        .with_context(|| "failed to connect Vara.Eth API client for predeployed checkers")?;

    log_step("Wait Predeployed Checkers");
    wait_for_programs_on_vara_eth(&api, &checker_ids).await?;

    for (index, checker_id) in checker_ids.iter().enumerate() {
        let executable_balance = executable_balance(&api, *checker_id).await?;
        info!(
            checker_index = index,
            %checker_id,
            executable_balance_vara = %format_vara(executable_balance),
            "Predeployed checker is available"
        );
    }

    setup_manager_with_checker_ids(&config, checker_ids).await
}

async fn wait_for_generation(
    api: &VaraEthApi,
    manager_id: ActorId,
    expected_points: u32,
) -> Result<()> {
    for _ in 0..MAX_POLL_ATTEMPTS {
        let points_sent = manager_points_sent(api, manager_id).await?;
        if points_sent > 0 || expected_points == 0 {
            return Ok(());
        }
        sleep(POLL_DELAY).await;
    }

    anyhow::bail!("manager did not start sending generated points in time");
}

async fn ensure_checking_started(
    api: &VaraEthApi,
    manager_id: ActorId,
    max_iter: u32,
    batch_size: u32,
) -> Result<()> {
    let manager_mirror = api.mirror(manager_id);
    let points_sent = manager_points_sent(api, manager_id).await?;
    if points_sent > 0 {
        info!(
            points_sent,
            "Manager already started sending batches to checkers"
        );
        return Ok(());
    }

    info!(
        max_iter,
        batch_size,
        "Manager has not started checking yet; sending CheckPointsSet manually"
    );

    let previous_state_hash = manager_mirror.state_hash().await?;
    let (receipt, message_id) = manager_mirror
        .send_message_with_receipt(
            manager_payload("CheckPointsSet", (max_iter, batch_size, false)),
            0,
        )
        .await
        .with_context(|| "failed to send manual CheckPointsSet to manager")?;
    assert!(receipt.status(), "expected successful CheckPointsSet receipt");

    let reply = manager_mirror
        .wait_for_reply(message_id)
        .await
        .with_context(|| "failed to wait for manual CheckPointsSet reply")?;
    assert_auto_success(&reply.code.to_bytes(), "manager check_points_set");
    wait_for_state_hash_change(api, manager_id, previous_state_hash).await?;

    let points_sent_after = manager_points_sent(api, manager_id).await?;
    info!(
        points_sent_before = points_sent,
        points_sent_after,
        "Manual CheckPointsSet finished"
    );

    Ok(())
}

async fn wait_for_completion(
    api: &VaraEthApi,
    manager_id: ActorId,
    checker_ids: &[ActorId],
    expected_points: u32,
) -> Result<()> {
    for _ in 0..MAX_POLL_ATTEMPTS * 10 {
        let points_sent = manager_points_sent(api, manager_id).await?;
        let checked_count = manager_checked_count(api, manager_id).await?;
        let manager_executable_balance = executable_balance(api, manager_id).await?;
        let checker_executable_balances = futures_util::future::try_join_all(
            checker_ids
                .iter()
                .copied()
                .map(|checker_id| async move {
                    executable_balance(api, checker_id).await
                }),
        )
        .await?;

        info!(
            expected_points,
            points_sent,
            checked_count,
            manager_executable_balance_vara = %format_vara(manager_executable_balance),
            checker_0_executable_balance_vara = %format_vara(checker_executable_balances[0]),
            checker_1_executable_balance_vara = %format_vara(checker_executable_balances[1]),
            checker_2_executable_balance_vara = %format_vara(checker_executable_balances[2]),
            "Manager stress progress"
        );

        if points_sent >= expected_points && checked_count == expected_points {
            return Ok(());
        }

        sleep(POLL_DELAY).await;
    }

    anyhow::bail!("manager did not finish checking all generated points in time");
}

#[tokio::test]
async fn deploy_prepared_checkers_for_manager_debug_on_testnet() -> Result<()> {
    init_tracing();
    log_step("Deploy Prepared Checkers");

    let config = TestConfig::load()?;
    let raw_ethereum = config.connect_ethereum().await?;
    let raw_api = VaraEthApi::new(&config.vara_eth_rpc, raw_ethereum.clone())
        .await
        .with_context(|| "failed to connect Vara.Eth API client")?;

    let checker_ids = deploy_and_prepare_checkers(&raw_api, &raw_ethereum, &config).await?;

    for (index, checker_id) in checker_ids.iter().enumerate() {
        info!(checker_index = index, %checker_id, "Prepared checker ready for reuse");
    }

    let checker_ids_for_copy = checker_ids
        .iter()
        .map(|checker_id| format!("    \"{checker_id}\","))
        .collect::<Vec<_>>()
        .join("\n");
    info!(
        checker_count = checker_ids.len(),
        checker_ids_for_copy = %checker_ids_for_copy,
        "Copy these addresses into PREDEPLOYED_CHECKER_IDS"
    );

    Ok(())
}

#[tokio::test]
async fn manager_runs_with_predeployed_checkers_on_testnet() -> Result<()> {
    init_tracing();

    log_step("Setup With Predeployed Checkers");
    let ctx = setup_manager_with_predeployed_checkers().await?;
    let manager_mirror = ctx.api.mirror(ctx.manager_id);

    for scenario in STRESS_SCENARIOS {
        log_step(scenario.name);

        let total_points = scenario.width * scenario.height;

        let manager_exec_before = manager_mirror.state().await?.executable_balance;
        let api = &ctx.api;
        let checker_exec_before: Vec<u128> = futures_util::future::try_join_all(
            ctx.checker_ids
                .iter()
                .copied()
                .map(|checker_id| async move {
                    Ok::<u128, anyhow::Error>(
                        api.mirror(checker_id).state().await?.executable_balance,
                    )
                }),
        )
        .await?;

        let previous_state_hash = manager_mirror.state_hash().await?;
        let (restart_receipt, restart_message_id) = manager_mirror
            .send_message_with_receipt(manager_payload("Restart", ()), 0)
            .await
            .with_context(|| "failed to send Restart to manager")?;
        assert!(restart_receipt.status(), "expected successful Restart receipt");
        let restart_reply = manager_mirror
            .wait_for_reply(restart_message_id)
            .await
            .with_context(|| "failed to wait for Restart reply")?;
        assert_auto_success(&restart_reply.code.to_bytes(), "manager restart");
        wait_for_state_hash_change(&ctx.api, ctx.manager_id, previous_state_hash).await?;

        let previous_state_hash = manager_mirror.state_hash().await?;
        let generate_payload = manager_payload(
            "GenerateAndStorePoints",
            (
                scenario.width,
                scenario.height,
                scenario.x_min_num,
                scenario.x_min_scale,
                scenario.x_max_num,
                scenario.x_max_scale,
                scenario.y_min_num,
                scenario.y_min_scale,
                scenario.y_max_num,
                scenario.y_max_scale,
                scenario.points_per_call,
                true,
                true,
                scenario.max_iter,
                scenario.batch_size,
            ),
        );
        let (generate_receipt, generate_message_id) = manager_mirror
            .send_message_with_receipt(generate_payload, 0)
            .await
            .with_context(|| format!("failed to start manager scenario {}", scenario.name))?;
        assert!(
            generate_receipt.status(),
            "expected successful GenerateAndStorePoints receipt"
        );
        let generate_reply = manager_mirror
            .wait_for_reply(generate_message_id)
            .await
            .with_context(|| "failed to wait for GenerateAndStorePoints reply")?;
        assert_auto_success(&generate_reply.code.to_bytes(), "manager generate_and_store_points");
        wait_for_state_hash_change(&ctx.api, ctx.manager_id, previous_state_hash).await?;

        wait_for_generation(&ctx.api, ctx.manager_id, total_points).await?;
        ensure_checking_started(&ctx.api, ctx.manager_id, scenario.max_iter, scenario.batch_size)
            .await?;
        wait_for_completion(&ctx.api, ctx.manager_id, &ctx.checker_ids, total_points).await?;

        let points_sent = manager_points_sent(&ctx.api, ctx.manager_id).await?;
        let checked_count = manager_checked_count(&ctx.api, ctx.manager_id).await?;
        let manager_exec_after = manager_mirror.state().await?.executable_balance;
        let api = &ctx.api;
        let checker_exec_after: Vec<u128> = futures_util::future::try_join_all(
            ctx.checker_ids
                .iter()
                .copied()
                .map(|checker_id| async move {
                    Ok::<u128, anyhow::Error>(
                        api.mirror(checker_id).state().await?.executable_balance,
                    )
                }),
        )
        .await?;

        assert_eq!(points_sent, total_points);
        assert_eq!(checked_count, total_points);

        info!(
            scenario = scenario.name,
            total_points,
            points_per_call = scenario.points_per_call,
            batch_size = scenario.batch_size,
            max_iter = scenario.max_iter,
            manager_exec_before_vara = %format_vara(manager_exec_before),
            manager_exec_after_vara = %format_vara(manager_exec_after),
            manager_exec_delta_vara = %format_vara(manager_exec_before.saturating_sub(manager_exec_after)),
            checker_0_exec_before_vara = %format_vara(checker_exec_before[0]),
            checker_0_exec_after_vara = %format_vara(checker_exec_after[0]),
            checker_1_exec_before_vara = %format_vara(checker_exec_before[1]),
            checker_1_exec_after_vara = %format_vara(checker_exec_after[1]),
            checker_2_exec_before_vara = %format_vara(checker_exec_before[2]),
            checker_2_exec_after_vara = %format_vara(checker_exec_after[2]),
            "Manager stress scenario with predeployed checkers completed"
        );
    }

    Ok(())
}

#[tokio::test]
async fn manager_distributes_points_across_three_checkers_on_testnet() -> Result<()> {
    init_tracing();

    log_step("Setup");
    let ctx = setup_manager_and_checkers().await?;
    let manager_mirror = ctx.api.mirror(ctx.manager_id);

    for scenario in STRESS_SCENARIOS {
        log_step(scenario.name);

        let total_points = scenario.width * scenario.height;

        let manager_exec_before = manager_mirror.state().await?.executable_balance;
        let api = &ctx.api;
        let checker_exec_before: Vec<u128> = futures_util::future::try_join_all(
            ctx.checker_ids
                .iter()
                .copied()
                .map(|checker_id| async move {
                    Ok::<u128, anyhow::Error>(
                        api.mirror(checker_id).state().await?.executable_balance,
                    )
                }),
        )
        .await?;

        let previous_state_hash = manager_mirror.state_hash().await?;
        let (restart_receipt, restart_message_id) = manager_mirror
            .send_message_with_receipt(manager_payload("Restart", ()), 0)
            .await
            .with_context(|| "failed to send Restart to manager")?;
        assert!(restart_receipt.status(), "expected successful Restart receipt");
        let restart_reply = manager_mirror
            .wait_for_reply(restart_message_id)
            .await
            .with_context(|| "failed to wait for Restart reply")?;
        assert_auto_success(&restart_reply.code.to_bytes(), "manager restart");
        wait_for_state_hash_change(&ctx.api, ctx.manager_id, previous_state_hash).await?;

        let previous_state_hash = manager_mirror.state_hash().await?;
        let generate_payload = manager_payload(
            "GenerateAndStorePoints",
            (
                scenario.width,
                scenario.height,
                scenario.x_min_num,
                scenario.x_min_scale,
                scenario.x_max_num,
                scenario.x_max_scale,
                scenario.y_min_num,
                scenario.y_min_scale,
                scenario.y_max_num,
                scenario.y_max_scale,
                scenario.points_per_call,
                true,
                true,
                scenario.max_iter,
                scenario.batch_size,
            ),
        );
        let (generate_receipt, generate_message_id) = manager_mirror
            .send_message_with_receipt(generate_payload, 0)
            .await
            .with_context(|| format!("failed to start manager scenario {}", scenario.name))?;
        assert!(
            generate_receipt.status(),
            "expected successful GenerateAndStorePoints receipt"
        );
        let generate_reply = manager_mirror
            .wait_for_reply(generate_message_id)
            .await
            .with_context(|| "failed to wait for GenerateAndStorePoints reply")?;
        assert_auto_success(&generate_reply.code.to_bytes(), "manager generate_and_store_points");
        wait_for_state_hash_change(&ctx.api, ctx.manager_id, previous_state_hash).await?;

        wait_for_generation(&ctx.api, ctx.manager_id, total_points).await?;
        ensure_checking_started(&ctx.api, ctx.manager_id, scenario.max_iter, scenario.batch_size)
            .await?;
        wait_for_completion(&ctx.api, ctx.manager_id, &ctx.checker_ids, total_points).await?;

        let points_sent = manager_points_sent(&ctx.api, ctx.manager_id).await?;
        let checked_count = manager_checked_count(&ctx.api, ctx.manager_id).await?;
        let manager_exec_after = manager_mirror.state().await?.executable_balance;
        let api = &ctx.api;
        let checker_exec_after: Vec<u128> = futures_util::future::try_join_all(
            ctx.checker_ids
                .iter()
                .copied()
                .map(|checker_id| async move {
                    Ok::<u128, anyhow::Error>(
                        api.mirror(checker_id).state().await?.executable_balance,
                    )
                }),
        )
        .await?;

        assert_eq!(points_sent, total_points);
        assert_eq!(checked_count, total_points);

        info!(
            scenario = scenario.name,
            total_points,
            points_per_call = scenario.points_per_call,
            batch_size = scenario.batch_size,
            max_iter = scenario.max_iter,
            manager_exec_before_vara = %format_vara(manager_exec_before),
            manager_exec_after_vara = %format_vara(manager_exec_after),
            manager_exec_delta_vara = %format_vara(manager_exec_before.saturating_sub(manager_exec_after)),
            checker_0_exec_before_vara = %format_vara(checker_exec_before[0]),
            checker_0_exec_after_vara = %format_vara(checker_exec_after[0]),
            checker_1_exec_before_vara = %format_vara(checker_exec_before[1]),
            checker_1_exec_after_vara = %format_vara(checker_exec_after[1]),
            checker_2_exec_before_vara = %format_vara(checker_exec_before[2]),
            checker_2_exec_after_vara = %format_vara(checker_exec_after[2]),
            "Manager stress scenario completed"
        );
    }

    Ok(())
}
