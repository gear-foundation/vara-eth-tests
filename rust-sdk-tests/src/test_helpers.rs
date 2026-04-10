use alloy::{rpc::types::TransactionReceipt, sol_types::SolEvent};
use anyhow::{Context, Result, anyhow};
use ethexe_ethereum::abi::IMirror;
use ethexe_sdk::VaraEthApi;
use gprimitives::{ActorId, H256, MessageId};
use tokio::time::{Duration, sleep};
use tracing::{info, instrument};

pub const MAX_POLL_ATTEMPTS: usize = 40;
pub const POLL_DELAY: Duration = Duration::from_secs(12);
pub const AUTO_SUCCESS_REPLY_CODE: [u8; 4] = [0, 0, 0, 0];
pub const MANUAL_SUCCESS_REPLY_CODE: [u8; 4] = [0, 1, 0, 0];
pub const USERSPACE_PANIC_REPLY_CODE: [u8; 4] = [1, 0, 3, 0];
pub const INITIALIZATION_FAILURE_REPLY_CODE: [u8; 4] = [1, 2, 1, 0];
pub const UNINITIALIZED_REPLY_CODE: [u8; 4] = [1, 2, 2, 0];

#[instrument(skip(api))]
pub async fn wait_for_program_on_vara_eth(api: &VaraEthApi, program_id: ActorId) -> Result<()> {
    for _ in 0..MAX_POLL_ATTEMPTS {
        let ids = api.router().program_ids().await?;
        info!(programs = ids.len(), "Polled Vara.Eth for program ids");
        if ids.contains(&program_id) {
            return Ok(());
        }
        sleep(POLL_DELAY).await;
    }

    Err(anyhow!(
        "program {program_id} did not appear on Vara.Eth within bounded polling window"
    ))
}

#[instrument(skip(api))]
pub async fn wait_for_executable_balance(
    api: &VaraEthApi,
    program_id: ActorId,
    expected_minimum: u128,
) -> Result<()> {
    let mirror = api.mirror(program_id);

    for _ in 0..MAX_POLL_ATTEMPTS {
        let state = mirror.state().await?;
        info!(
            executable_balance = state.executable_balance,
            "Polled mirror for executable balance"
        );
        if state.executable_balance >= expected_minimum {
            return Ok(());
        }
        sleep(POLL_DELAY).await;
    }

    Err(anyhow!(
        "program {program_id} executable balance did not reach {expected_minimum}"
    ))
}

#[instrument(skip(api))]
pub async fn top_up_executable_balance(
    api: &VaraEthApi,
    program_id: ActorId,
    amount: u128,
) -> Result<()> {
    let balance_before = api.mirror(program_id).state().await?.executable_balance;
    let approve_receipt = api
        .wrapped_vara()
        .approve_with_receipt(program_id, amount)
        .await
        .with_context(|| "failed to approve WVARA for executable balance top-up")?;
    assert!(
        approve_receipt.status(),
        "expected successful WVARA approve receipt for executable balance top-up"
    );
    assert!(
        approve_receipt.block_hash.is_some(),
        "expected block hash in WVARA approve receipt for executable balance top-up"
    );

    let top_up_receipt = api
        .mirror(program_id)
        .executable_balance_top_up_with_receipt(amount)
        .await
        .with_context(|| "failed to top up executable balance")?;
    assert!(
        top_up_receipt.status(),
        "expected successful executable balance top-up receipt"
    );
    assert!(
        top_up_receipt.block_hash.is_some(),
        "expected block hash in executable balance top-up receipt"
    );

    wait_for_executable_balance(api, program_id, balance_before.saturating_add(amount)).await
}

#[instrument(skip(api))]
pub async fn wait_for_state_hash_change(
    api: &VaraEthApi,
    program_id: ActorId,
    previous_state_hash: H256,
) -> Result<H256> {
    let mirror = api.mirror(program_id);

    for _ in 0..MAX_POLL_ATTEMPTS {
        let current_state_hash = mirror.state_hash().await?;
        info!(%current_state_hash, "Polled mirror state hash");
        if current_state_hash != previous_state_hash {
            return Ok(current_state_hash);
        }
        sleep(POLL_DELAY).await;
    }

    Err(anyhow!(
        "program {program_id} state hash did not change within bounded polling window"
    ))
}

pub fn extract_message_id_from_receipt(receipt: &TransactionReceipt) -> Result<MessageId> {
    receipt
        .inner
        .logs()
        .iter()
        .find_map(|log| IMirror::MessageQueueingRequested::decode_log(log.as_ref()).ok())
        .map(|event| (*event.id).into())
        .ok_or_else(|| anyhow!("couldn't find `MessageQueueingRequested` log in receipt"))
}

pub fn describe_reply_code(bytes: &[u8; 4]) -> String {
    match bytes {
        [0, 0, 0, 0] => "success: auto reply".to_string(),
        [0, 1, 0, 0] => "success: manual reply".to_string(),
        [1, 0, 0, 0] => "error: execution: ran out of gas".to_string(),
        [1, 0, 1, 0] => "error: execution: memory overflow".to_string(),
        [1, 0, 2, 0] => "error: execution: backend error".to_string(),
        [1, 0, 3, 0] => "error: execution: userspace panic".to_string(),
        [1, 0, 4, 0] => "error: execution: unreachable instruction".to_string(),
        [1, 0, 5, 0] => "error: execution: stack limit exceeded".to_string(),
        [1, 2, 0, 0] => "error: unavailable actor: program exited".to_string(),
        [1, 2, 1, 0] => "error: unavailable actor: initialization failure".to_string(),
        [1, 2, 2, 0] => "error: unavailable actor: uninitialized".to_string(),
        [1, 2, 3, 0] => "error: unavailable actor: program not created".to_string(),
        [1, 2, 4, 0] => "error: unavailable actor: reinstrumentation failure".to_string(),
        [1, 3, 0, 0] => "error: removed from waitlist".to_string(),
        _ => format!("unknown reply code {:?}", bytes),
    }
}

pub fn assert_auto_success(bytes: &[u8; 4], scope: &str) {
    assert_eq!(
        *bytes,
        AUTO_SUCCESS_REPLY_CODE,
        "expected auto success reply code for {scope}, got {bytes:?}",
    );
}

pub fn assert_manual_success(bytes: &[u8; 4], scope: &str) {
    assert_eq!(
        *bytes,
        MANUAL_SUCCESS_REPLY_CODE,
        "expected manual success reply code for {scope}, got {bytes:?} ({})",
        describe_reply_code(bytes),
    );
}

pub fn assert_reply_code(bytes: &[u8; 4], expected: [u8; 4], scope: &str) {
    assert_eq!(
        *bytes,
        expected,
        "unexpected reply code for {scope}: got {bytes:?} ({}), expected {:?} ({})",
        describe_reply_code(bytes),
        expected,
        describe_reply_code(&expected),
    );
}

pub fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_test_writer()
        .try_init();
}

pub fn log_step(step: &str) {
    info!("=== {step} ===");
}
