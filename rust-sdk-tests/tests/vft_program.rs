use alloy::{
    primitives::U256 as AlloyU256,
    providers::Provider,
};
use anyhow::{Context, Result, anyhow, ensure};
use futures_util::StreamExt;
use parity_scale_codec::{Decode, Encode};
use rust_sdk_tests::{
    config::TestConfig,
    test_helpers::{
        INITIALIZATION_FAILURE_REPLY_CODE, MANUAL_SUCCESS_REPLY_CODE, MAX_POLL_ATTEMPTS,
        POLL_DELAY, USERSPACE_PANIC_REPLY_CODE, assert_auto_success, assert_manual_success,
        assert_reply_code, describe_reply_code, extract_message_id_from_receipt, init_tracing,
        log_step, top_up_executable_balance, wait_for_executable_balance,
        wait_for_program_on_vara_eth, wait_for_state_hash_change,
    },
};
use std::{collections::HashSet, fs, path::Path};
use tokio::{
    time::{Duration, sleep, timeout},
};
use tracing::{info, instrument};

use ethexe_common::gear::CodeState;
use ethexe_ethereum::{TryGetReceipt, abi::IMirror};
use ethexe_sdk::VaraEthApi;
use gear_core::ids::prelude::CodeIdExt as _;
use gprimitives::{ActorId, CodeId, H256, U256};

const VFT_WASM_PATH: &str = "../target/wasm32-gear/release/extended_vft.opt.wasm";
const CODE_VALIDATION_TIMEOUT: Duration = Duration::from_secs(180);
const EVENT_TIMEOUT: Duration = Duration::from_secs(60);
// 5 Vara
const TOP_UP_AMOUNT: u128 = 5 * 1_000_000_000_000;
const BURST_TOP_UP_AMOUNT: u128 = 25 * 1_000_000_000_000;
const OWNED_TOP_UP_AMOUNT: u128 = 1_000_000_000;
const MIRROR_BURST_SIZE: usize = 100;
const MIRROR_BURST_TRANSFER_AMOUNT: u128 = 1;
const INJECTED_BURST_SIZE: usize = 100;
const INJECTED_BURST_TRANSFER_AMOUNT: u128 = 1;
const MIRROR_MINT_AMOUNT: u128 = 1_000;
const MIRROR_TRANSFER_AMOUNT: u128 = 250;
const INJECTED_TRANSFER_AMOUNT: u128 = 50;

struct TestVftContext {
    api: VaraEthApi,
    program_id: ActorId,
    code_id: CodeId,
    owner_actor_id: ActorId,
}

struct MetadataSnapshot {
    name: String,
    symbol: String,
    decimals: u16,
    executable_balance: u128,
}

struct MirrorIntrospectionSnapshot {
    balance: u128,
    executable_balance: u128,
    nonce: U256,
}

struct ScenarioState {
    recipient: ActorId,
    state_hash: H256,
    nonce_before_writes: U256,
    nonce_after_mirror_flow: Option<U256>,
    owned_top_up_block_hash: Option<H256>,
}

#[instrument(skip_all)]
async fn deploy_vft_program(init_program: bool) -> Result<TestVftContext> {
    let config = TestConfig::load()?;
    let api = config.connect_api().await?;
    let router = api.router();
    let owner_actor_id = config.signer_and_address()?.1.into();

    let code =
        fs::read(Path::new(VFT_WASM_PATH)).with_context(|| format!("failed to read {VFT_WASM_PATH}"))?;
    let code_id = CodeId::generate(&code);

    let code_state = router
        .code_state(code_id)
        .await
        .with_context(|| "failed to query VFT code state")?;

    info!(?code_state, "VFT code state");
    if matches!(code_state, CodeState::Validated) {
        info!("VFT code is already validated, reusing existing code id");
    } else {
        info!("Requesting code validation for VFT program");
        let (_receipt, requested_code_id) = router
            .request_code_validation_with_receipt(&code)
            .await
            .with_context(|| "failed to request VFT code validation")?;

        ensure!(
            requested_code_id == code_id,
            "requested code id does not match locally generated code id"
        );

        info!("Waiting for VFT code validation, this may take a few minutes...");
        let validation = timeout(CODE_VALIDATION_TIMEOUT, router.wait_for_code_validation(code_id))
            .await
            .with_context(|| "timed out while waiting for VFT code validation")??;

        ensure!(validation.valid, "VFT code validation returned invalid");
    }

    info!("Code validation successful, deploying VFT program");
    let salt = H256::random();
    let (receipt, program_id) = router
        .create_program_with_receipt(code_id, salt, None)
        .await
        .with_context(|| "failed to create VFT program")?;

    assert!(receipt.status(), "expected successful Ethereum receipt");
    assert!(
        receipt.block_hash.is_some(),
        "expected block hash in createProgram receipt"
    );
    assert_ne!(
        program_id,
        ActorId::zero(),
        "expected non-zero program id after createProgram"
    );

    info!(program_id = %program_id, "Program created, waiting for it to appear on Vara.Eth");

    wait_for_program_on_vara_eth(&api, program_id).await?;
    
    info!("Program appeared on Vara.Eth, topping up executable balance and initializing");

    let wvara = api.wrapped_vara();
    let approve_receipt = wvara
        .approve_with_receipt(program_id, TOP_UP_AMOUNT)
        .await
        .with_context(|| "failed to approve WVARA for VFT program")?;
    assert!(
        approve_receipt.status(),
        "expected successful WVARA approve receipt"
    );
    assert!(
        approve_receipt.block_hash.is_some(),
        "expected block hash in WVARA approve receipt"
    );

    info!("Approved WVARA for transfer, sending top-up transaction");

    let mirror = api.mirror(program_id);
    let top_up_receipt = mirror
        .executable_balance_top_up_with_receipt(TOP_UP_AMOUNT)
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

    info!("Top-up transaction sent, waiting for executable balance to reflect the top-up");

    wait_for_executable_balance(&api, program_id, TOP_UP_AMOUNT).await?;

    if !init_program {
        info!("Skipping init for VFT program");
        return Ok(TestVftContext {
            api,
            program_id,
            code_id,
            owner_actor_id,
        });
    }

    info!("Executable balance updated, sending init message to VFT program");
    let (init_receipt, init_message_id) = mirror
        .send_message_with_receipt(ctor_init_payload("Name", "Symbol", 12), 0)
        .await
        .with_context(|| "failed to send VFT init message")?;
    assert!(init_receipt.status(), "expected successful init message receipt");
    assert!(
        init_receipt.block_hash.is_some(),
        "expected block hash in init message receipt"
    );

    info!("Init message sent, waiting for init reply from VFT program");
    let reply = mirror
        .wait_for_reply(init_message_id)
        .await
        .with_context(|| "failed to wait for VFT init reply")?;

    assert_auto_success(&reply.code.to_bytes(), "init");

    info!("VFT program initialized successfully");
    Ok(TestVftContext {
        api,
        program_id,
        code_id,
        owner_actor_id,
    })
}

#[instrument(skip_all)]
async fn setup_vft_program() -> Result<TestVftContext> {
    deploy_vft_program(true).await
}

#[instrument(skip(api))]
async fn executable_balance(api: &VaraEthApi, program_id: ActorId) -> Result<u128> {
    Ok(api.mirror(program_id).state().await?.executable_balance)
}

fn decode_sails_reply<T: Decode>(
    payload: &[u8],
    expected_service: &str,
    expected_action: &str,
) -> Result<T> {
    let (service, action, value) = <(String, String, T)>::decode(&mut payload.as_ref())
        .with_context(|| "failed to decode Sails reply envelope")?;

    assert_eq!(service, expected_service);
    assert_eq!(action, expected_action);

    Ok(value)
}

async fn query_at<T: Decode>(
    api: &VaraEthApi,
    program_id: ActorId,
    action: &str,
    payload: Vec<u8>,
    at: Option<H256>,
) -> Result<T> {
    let reply = api
        .mirror(program_id)
        .calculate_reply_for_handle_at(payload, 0, at)
        .await
        .with_context(|| "failed to calculate reply for handle query")?;

    assert_manual_success(&reply.code.to_bytes(), "query");
    decode_sails_reply::<T>(&reply.payload, "Vft", action)
}

async fn query<T: Decode>(
    api: &VaraEthApi,
    program_id: ActorId,
    action: &str,
    payload: Vec<u8>,
) -> Result<T> {
    query_at(api, program_id, action, payload, None).await
}

#[instrument(skip(api))]
async fn balance_of(api: &VaraEthApi, program_id: ActorId, account: ActorId) -> Result<u128> {
    query::<String>(api, program_id, "BalanceOf", vft_payload("BalanceOf", (account,)))
        .await?
        .parse::<u128>()
        .with_context(|| "failed to parse BalanceOf result as u128")
}

#[instrument(skip(api))]
async fn total_supply(api: &VaraEthApi, program_id: ActorId) -> Result<u128> {
    query::<String>(api, program_id, "TotalSupply", vft_payload("TotalSupply", ()))
        .await?
        .parse::<u128>()
        .with_context(|| "failed to parse TotalSupply result as u128")
}

async fn query_metadata(api: &VaraEthApi, program_id: ActorId) -> Result<MetadataSnapshot> {
    Ok(MetadataSnapshot {
        name: query::<String>(api, program_id, "Name", vft_payload("Name", ())).await?,
        symbol: query::<String>(api, program_id, "Symbol", vft_payload("Symbol", ())).await?,
        decimals: query::<u16>(api, program_id, "Decimals", vft_payload("Decimals", ())).await?,
        executable_balance: executable_balance(api, program_id).await?,
    })
}

fn assert_metadata(ctx: &TestVftContext, metadata: &MetadataSnapshot) {
    assert_ne!(
        ctx.code_id.to_string(),
        "0x0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert_eq!(metadata.name, "Name");
    assert_eq!(metadata.symbol, "Symbol");
    assert_eq!(metadata.decimals, 12);
}

async fn inspect_mirror(
    api: &VaraEthApi,
    program_id: ActorId,
    owner_actor_id: ActorId,
) -> Result<MirrorIntrospectionSnapshot> {
    let mirror = api.mirror(program_id);
    let initializer = mirror.initializer().await?;
    let state = mirror.state().await?;
    let full_state = mirror.full_state().await?;
    let nonce = mirror.nonce().await?;

    assert_eq!(initializer, owner_actor_id);
    assert_eq!(state.balance, full_state.balance);
    assert_eq!(state.executable_balance, full_state.executable_balance);
    assert!(
        !state.requires_init_message(),
        "initialized program should no longer require init message"
    );
    info!(
        %initializer,
        nonce = %nonce,
        balance = state.balance,
        executable_balance = state.executable_balance,
        "Mirror introspection before mutations"
    );

    Ok(MirrorIntrospectionSnapshot {
        balance: state.balance,
        executable_balance: state.executable_balance,
        nonce,
    })
}

async fn assert_read_only_query_path(
    api: &VaraEthApi,
    program_id: ActorId,
) -> Result<H256> {
    let mirror = api.mirror(program_id);
    let state_hash_before_queries = mirror.state_hash().await?;
    info!(
        %state_hash_before_queries,
        "State hash before read-only queries"
    );

    let queried_name = query::<String>(api, program_id, "Name", vft_payload("Name", ())).await?;
    let queried_symbol =
        query::<String>(api, program_id, "Symbol", vft_payload("Symbol", ())).await?;
    let queried_decimals =
        query::<u16>(api, program_id, "Decimals", vft_payload("Decimals", ())).await?;

    let state_hash_after_queries = mirror.state_hash().await?;
    info!(
        %state_hash_after_queries,
        "State hash after read-only queries"
    );

    assert_eq!(queried_name, "Name");
    assert_eq!(queried_symbol, "Symbol");
    assert_eq!(queried_decimals, 12);
    assert_eq!(
        state_hash_before_queries, state_hash_after_queries,
        "read-only queries should not force a state hash change"
    );

    Ok(state_hash_after_queries)
}

async fn assert_final_state(
    api: &VaraEthApi,
    program_id: ActorId,
    owner_actor_id: ActorId,
    recipient: ActorId,
) -> Result<()> {
    let owner_balance = balance_of(api, program_id, owner_actor_id).await?;
    let recipient_balance = balance_of(api, program_id, recipient).await?;
    let supply = total_supply(api, program_id).await?;
    let current_executable_balance = executable_balance(api, program_id).await?;

    assert_eq!(
        owner_balance,
        MIRROR_MINT_AMOUNT - MIRROR_TRANSFER_AMOUNT + MIRROR_MINT_AMOUNT - INJECTED_TRANSFER_AMOUNT
    );
    assert_eq!(recipient_balance, MIRROR_TRANSFER_AMOUNT + INJECTED_TRANSFER_AMOUNT);
    assert_eq!(supply, MIRROR_MINT_AMOUNT + MIRROR_MINT_AMOUNT);
    assert!(
        current_executable_balance > 0,
        "expected executable balance to remain on the active program after injected calls"
    );

    info!(
        owner_balance,
        recipient_balance,
        total_supply = supply,
        executable_balance = current_executable_balance,
        "Final token state"
    );

    Ok(())
}

async fn wait_for_expected_token_state(
    api: &VaraEthApi,
    program_id: ActorId,
    owner_actor_id: ActorId,
    recipient: ActorId,
    previous_state_hash: H256,
    expected_owner_balance: u128,
    expected_recipient_balance: u128,
    expected_total_supply: u128,
) -> Result<H256> {
    let mirror = api.mirror(program_id);

    for _ in 0..MAX_POLL_ATTEMPTS {
        let current_state_hash = mirror.state_hash().await?;
        let owner_balance = balance_of(api, program_id, owner_actor_id).await?;
        let recipient_balance = balance_of(api, program_id, recipient).await?;
        let total_supply = total_supply(api, program_id).await?;

        info!(
            %current_state_hash,
            owner_balance,
            recipient_balance,
            total_supply,
            "Polling final token state after burst"
        );

        if current_state_hash != previous_state_hash
            && owner_balance == expected_owner_balance
            && recipient_balance == expected_recipient_balance
            && total_supply == expected_total_supply
        {
            return Ok(current_state_hash);
        }

        sleep(POLL_DELAY).await;
    }

    Err(anyhow!(
        "program {program_id} did not reach expected token state within bounded polling window"
    ))
}

async fn mirror_mint_with_state_change(
    ctx: &TestVftContext,
    amount: u128,
) -> Result<H256> {
    let mirror = ctx.api.mirror(ctx.program_id);
    let previous_state_hash = mirror.state_hash().await?;
    let (receipt, message_id) = mirror
        .send_message_with_receipt(
            vft_payload("Mint", (ctx.owner_actor_id, amount.to_string())),
            0,
        )
        .await
        .with_context(|| "failed to send mirror mint message")?;
    assert!(receipt.status(), "expected successful mirror mint receipt");
    assert!(
        receipt.block_hash.is_some(),
        "expected block hash in mirror mint receipt"
    );

    let reply = mirror
        .wait_for_reply(message_id)
        .await
        .with_context(|| "failed to wait for mirror mint reply")?;
    assert_manual_success(&reply.code.to_bytes(), "mirror mint");
    assert!(decode_sails_reply::<bool>(&reply.payload, "Vft", "Mint")?);

    wait_for_state_hash_change(&ctx.api, ctx.program_id, previous_state_hash).await
}

async fn assert_initial_queries(ctx: &TestVftContext) -> Result<()> {
    let metadata = query_metadata(&ctx.api, ctx.program_id).await?;
    assert_metadata(ctx, &metadata);
    info!(
        current_executable_balance = metadata.executable_balance,
        "Executable balance after initialization"
    );
    Ok(())
}

fn scenario_from_introspection(
    mirror_snapshot: &MirrorIntrospectionSnapshot,
    state_hash: H256,
) -> ScenarioState {
    ScenarioState {
        recipient: ActorId::from(42_u64),
        state_hash,
        nonce_before_writes: mirror_snapshot.nonce,
        nonce_after_mirror_flow: None,
        owned_top_up_block_hash: None,
    }
}

async fn run_owned_balance_path(
    ctx: &TestVftContext,
    mirror_snapshot: &MirrorIntrospectionSnapshot,
    mut scenario: ScenarioState,
) -> Result<ScenarioState> {
    log_step("Owned Balance Path");
    let mirror = ctx.api.mirror(ctx.program_id);
    let owned_balance_before = mirror_snapshot.balance;
    let executable_balance_before_owned_top_up = mirror_snapshot.executable_balance;
    let mut owned_top_up_events = mirror
        .events()
        .owned_balance_top_up_requested()
        .subscribe()
        .await?;

    let owned_top_up_receipt = mirror
        .owned_balance_top_up_with_receipt(OWNED_TOP_UP_AMOUNT)
        .await
        .with_context(|| "failed to top up owned balance")?;
    assert!(
        owned_top_up_receipt.status(),
        "expected successful owned balance top-up receipt"
    );
    assert!(
        owned_top_up_receipt.block_hash.is_some(),
        "expected block hash in owned balance top-up receipt"
    );

    let owned_top_up_event = timeout(EVENT_TIMEOUT, owned_top_up_events.next())
        .await
        .with_context(|| "timed out while waiting for owned balance top-up event")?
        .ok_or_else(|| anyhow!("owned balance top-up event stream ended unexpectedly"))??;
    let (owned_top_up_event, _) = owned_top_up_event;
    assert_eq!(owned_top_up_event.value, OWNED_TOP_UP_AMOUNT);

    let owned_top_up_block_hash: H256 = (*(*owned_top_up_receipt
        .block_hash
        .as_ref()
        .expect("asserted above")))
    .into();

    scenario.state_hash =
        wait_for_state_hash_change(&ctx.api, ctx.program_id, scenario.state_hash).await?;
    scenario.owned_top_up_block_hash = Some(owned_top_up_block_hash);

    let state_after_owned_top_up = mirror.state().await?;
    let full_state_after_owned_top_up = mirror.full_state().await?;

    assert_eq!(
        state_after_owned_top_up.balance,
        owned_balance_before + OWNED_TOP_UP_AMOUNT
    );
    assert_eq!(
        state_after_owned_top_up.executable_balance,
        executable_balance_before_owned_top_up
    );
    assert_eq!(
        state_after_owned_top_up.balance,
        full_state_after_owned_top_up.balance
    );
    assert_eq!(
        state_after_owned_top_up.executable_balance,
        full_state_after_owned_top_up.executable_balance
    );
    info!(
        value = owned_top_up_event.value,
        %scenario.state_hash,
        balance = state_after_owned_top_up.balance,
        executable_balance = state_after_owned_top_up.executable_balance,
        "Owned balance top-up updated reducible balance without touching executable balance"
    );

    Ok(scenario)
}

async fn run_mirror_transport_path(
    ctx: &TestVftContext,
    mut scenario: ScenarioState,
) -> Result<ScenarioState> {
    log_step("Mirror Transport Path");
    let mirror = ctx.api.mirror(ctx.program_id);
    info!(%scenario.state_hash, "Initial state hash before mirror transport path");
    let mut message_queueing_events = mirror.events().message_queueing_requested().subscribe().await?;
    let mut reply_events = mirror.events().reply().subscribe().await?;
    let mut state_changed_events = mirror.events().state_changed().subscribe().await?;

    info!(amount = MIRROR_MINT_AMOUNT, "Sending mirror mint");
    let (mint_receipt, mint_message_id) = mirror
        .send_message_with_receipt(
            vft_payload("Mint", (ctx.owner_actor_id, MIRROR_MINT_AMOUNT.to_string())),
            0,
        )
        .await
        .with_context(|| "failed to send mirror mint message")?;
    assert!(mint_receipt.status(), "expected successful mirror mint receipt");
    assert!(
        mint_receipt.block_hash.is_some(),
        "expected block hash in mirror mint receipt"
    );
    info!(
        tx_hash = %mint_receipt.transaction_hash,
        message_id = %mint_message_id,
        "Mirror mint sent"
    );

    let message_queueing_event = timeout(EVENT_TIMEOUT, message_queueing_events.next())
        .await
        .with_context(|| "timed out while waiting for message queueing event")?
        .ok_or_else(|| anyhow!("message queueing event stream ended unexpectedly"))??;
    let (message_queueing_event, _) = message_queueing_event;
    assert_eq!(message_queueing_event.id, mint_message_id);
    assert_eq!(message_queueing_event.value, 0);
    info!(
        message_id = %message_queueing_event.id,
        "Observed MessageQueueingRequested event for mirror mint"
    );

    let mint_reply = mirror
        .wait_for_reply(mint_message_id)
        .await
        .with_context(|| "failed to wait for mirror mint reply")?;

    assert_manual_success(&mint_reply.code.to_bytes(), "mirror mint");
    assert!(decode_sails_reply::<bool>(&mint_reply.payload, "Vft", "Mint")?);
    info!(reply_code = ?mint_reply.code.to_bytes(), "Mirror mint reply received");
    let reply_event = timeout(EVENT_TIMEOUT, reply_events.next())
        .await
        .with_context(|| "timed out while waiting for mirror reply event")?
        .ok_or_else(|| anyhow!("reply event stream ended unexpectedly"))??;
    let (reply_event, _) = reply_event;
    assert_eq!(reply_event.reply_to, mint_message_id);
    assert_eq!(reply_event.reply_code.to_bytes(), MANUAL_SUCCESS_REPLY_CODE);
    scenario.state_hash =
        wait_for_state_hash_change(&ctx.api, ctx.program_id, scenario.state_hash).await?;
    let state_changed_event = timeout(EVENT_TIMEOUT, state_changed_events.next())
        .await
        .with_context(|| "timed out while waiting for state changed event")?
        .ok_or_else(|| anyhow!("state changed event stream ended unexpectedly"))??;
    let (state_changed_event, _) = state_changed_event;
    assert_eq!(state_changed_event.state_hash, scenario.state_hash);
    info!(%scenario.state_hash, "Observed new state hash after mirror mint");

    let historical_supply_before_mint = query_at::<String>(
        &ctx.api,
        ctx.program_id,
        "TotalSupply",
        vft_payload("TotalSupply", ()),
        scenario.owned_top_up_block_hash,
    )
    .await?;
    let latest_supply_after_mint = total_supply(&ctx.api, ctx.program_id).await?;
    assert_eq!(historical_supply_before_mint, "0");
    assert_eq!(latest_supply_after_mint, MIRROR_MINT_AMOUNT);
    info!(
        historical_supply_before_mint,
        latest_supply_after_mint,
        "Historical reply path can evaluate supply at an older state context"
    );

    info!(to = %scenario.recipient, amount = MIRROR_TRANSFER_AMOUNT, "Sending mirror transfer");
    let (transfer_receipt, transfer_message_id) = mirror
        .send_message_with_receipt(
            vft_payload("Transfer", (scenario.recipient, MIRROR_TRANSFER_AMOUNT.to_string())),
            0,
        )
        .await
        .with_context(|| "failed to send mirror transfer message")?;
    assert!(
        transfer_receipt.status(),
        "expected successful mirror transfer receipt"
    );
    assert!(
        transfer_receipt.block_hash.is_some(),
        "expected block hash in mirror transfer receipt"
    );
    info!(
        tx_hash = %transfer_receipt.transaction_hash,
        message_id = %transfer_message_id,
        "Mirror transfer sent"
    );

    let transfer_reply = mirror
        .wait_for_reply(transfer_message_id)
        .await
        .with_context(|| "failed to wait for mirror transfer reply")?;

    assert_manual_success(&transfer_reply.code.to_bytes(), "mirror transfer");
    assert!(decode_sails_reply::<bool>(&transfer_reply.payload, "Vft", "Transfer")?);
    info!(reply_code = ?transfer_reply.code.to_bytes(), "Mirror transfer reply received");
    scenario.state_hash =
        wait_for_state_hash_change(&ctx.api, ctx.program_id, scenario.state_hash).await?;
    info!(%scenario.state_hash, "Observed new state hash after mirror transfer");
    let nonce_after_mirror_flow = mirror.nonce().await?;
    assert!(
        nonce_after_mirror_flow > scenario.nonce_before_writes,
        "expected mirror nonce to grow after canonical mirror messages"
    );
    scenario.nonce_after_mirror_flow = Some(nonce_after_mirror_flow);
    info!(
        nonce_before = %scenario.nonce_before_writes,
        nonce_after = %nonce_after_mirror_flow,
        "Mirror nonce advanced after canonical mirror flow"
    );

    let owner_balance = balance_of(&ctx.api, ctx.program_id, ctx.owner_actor_id).await?;
    let recipient_balance = balance_of(&ctx.api, ctx.program_id, scenario.recipient).await?;
    let supply = total_supply(&ctx.api, ctx.program_id).await?;
    let current_executable_balance = executable_balance(&ctx.api, ctx.program_id).await?;

    assert_eq!(owner_balance, MIRROR_MINT_AMOUNT - MIRROR_TRANSFER_AMOUNT);
    assert_eq!(recipient_balance, MIRROR_TRANSFER_AMOUNT);
    assert_eq!(supply, MIRROR_MINT_AMOUNT);
    assert!(
        current_executable_balance > 0,
        "expected executable balance to remain on the active program after mirror calls"
    );
    info!(
        owner_balance,
        recipient_balance,
        total_supply = supply,
        executable_balance = current_executable_balance,
        "Mirror path left the node queryable with observable state"
    );

    Ok(scenario)
}

async fn run_injected_transport_path(
    ctx: &TestVftContext,
    mut scenario: ScenarioState,
) -> Result<ScenarioState> {
    log_step("Injected Transport Path");
    let mirror = ctx.api.mirror(ctx.program_id);

    info!(amount = MIRROR_MINT_AMOUNT, "Sending injected mint");
    let (_message_id, mint_promise) = mirror
        .send_message_injected_and_watch(
            vft_payload("Mint", (ctx.owner_actor_id, MIRROR_MINT_AMOUNT.to_string())),
            0,
        )
        .await
        .with_context(|| "failed to send injected mint message")?;

    assert_manual_success(&mint_promise.reply.code.to_bytes(), "injected mint");
    assert!(decode_sails_reply::<bool>(&mint_promise.reply.payload, "Vft", "Mint")?);
    info!(reply_code = ?mint_promise.reply.code.to_bytes(), "Injected mint promise received");
    scenario.state_hash =
        wait_for_state_hash_change(&ctx.api, ctx.program_id, scenario.state_hash).await?;
    info!(%scenario.state_hash, "Observed new state hash after injected mint");

    info!(to = %scenario.recipient, amount = INJECTED_TRANSFER_AMOUNT, "Sending injected transfer");
    let (_message_id, transfer_promise) = mirror
        .send_message_injected_and_watch(
            vft_payload("Transfer", (scenario.recipient, INJECTED_TRANSFER_AMOUNT.to_string())),
            0,
        )
        .await
        .with_context(|| "failed to send injected transfer message")?;

    assert_manual_success(&transfer_promise.reply.code.to_bytes(), "injected transfer");
    assert!(decode_sails_reply::<bool>(&transfer_promise.reply.payload, "Vft", "Transfer")?);
    info!(
        reply_code = ?transfer_promise.reply.code.to_bytes(),
        "Injected transfer promise received"
    );
    scenario.state_hash =
        wait_for_state_hash_change(&ctx.api, ctx.program_id, scenario.state_hash).await?;
    info!(%scenario.state_hash, "Observed new state hash after injected transfer");

    let nonce_after_injected_flow = mirror.nonce().await?;
    assert_eq!(
        Some(nonce_after_injected_flow),
        scenario.nonce_after_mirror_flow,
        "injected path should not advance mirror nonce"
    );
    info!(
        nonce_after_mirror = %scenario.nonce_after_mirror_flow.expect("set by mirror path"),
        nonce_after_injected = %nonce_after_injected_flow,
        "Injected path changed state without consuming mirror nonce"
    );

    Ok(scenario)
}

fn ctor_init_payload(name: &str, symbol: &str, decimals: u16) -> Vec<u8> {
    ["Init".to_string().encode(), (name.to_string(), symbol.to_string(), decimals).encode()].concat()
}

fn vft_payload<T: Encode>(action: &str, args: T) -> Vec<u8> {
    ["Vft".to_string().encode(), action.to_string().encode(), args.encode()].concat()
}

#[tokio::test]
async fn vft_full_lifecycle_on_testnet() -> Result<()> {
    init_tracing();
    log_step("Setup");
    let ctx = setup_vft_program().await?;
    assert_initial_queries(&ctx).await?;

    log_step("Mirror Introspection");
    let mirror_snapshot = inspect_mirror(&ctx.api, ctx.program_id, ctx.owner_actor_id).await?;

    log_step("Read-Only Query Path");
    let state_hash_after_queries = assert_read_only_query_path(&ctx.api, ctx.program_id).await?;

    let scenario = scenario_from_introspection(&mirror_snapshot, state_hash_after_queries);
    let scenario = run_owned_balance_path(&ctx, &mirror_snapshot, scenario).await?;
    let scenario = run_mirror_transport_path(&ctx, scenario).await?;
    let scenario = run_injected_transport_path(&ctx, scenario).await?;

    log_step("Final State");
    assert_final_state(&ctx.api, ctx.program_id, ctx.owner_actor_id, scenario.recipient).await
}

#[tokio::test]
async fn vft_mirror_parallel_burst_on_testnet() -> Result<()> {
    init_tracing();
    log_step("Setup");
    let ctx = setup_vft_program().await?;
    let mirror = ctx.api.mirror(ctx.program_id);
    let recipient = ActorId::from(77_u64);
    let burst_total_amount = MIRROR_BURST_SIZE as u128 * MIRROR_BURST_TRANSFER_AMOUNT;

    log_step("Initial Mint");
    let burst_start_state_hash = mirror_mint_with_state_change(&ctx, burst_total_amount).await?;
    let owner_balance_before = balance_of(&ctx.api, ctx.program_id, ctx.owner_actor_id).await?;
    let recipient_balance_before = balance_of(&ctx.api, ctx.program_id, recipient).await?;
    let total_supply_before = total_supply(&ctx.api, ctx.program_id).await?;
    let mirror_nonce_before = mirror.nonce().await?;

    assert_eq!(owner_balance_before, burst_total_amount);
    assert_eq!(recipient_balance_before, 0);
    assert_eq!(total_supply_before, burst_total_amount);

    log_step("Burst Top Up");
    info!(
        additional_top_up = BURST_TOP_UP_AMOUNT,
        "Adding extra executable balance for parallel burst"
    );
    top_up_executable_balance(&ctx.api, ctx.program_id, BURST_TOP_UP_AMOUNT).await?;
    let burst_executable_balance = executable_balance(&ctx.api, ctx.program_id).await?;
    info!(
        burst_executable_balance,
        "Executable balance before parallel burst"
    );

    log_step("Parallel Mirror Burst");
    let config = TestConfig::load()?;
    let ethereum = config.connect_ethereum().await?;
    let provider = ethereum.provider();
    let sender_address = ethereum.sender_address();
    let base_account_nonce = provider.get_transaction_count(sender_address.into()).pending().await?;
    let mirror_address = ctx.program_id.to_address_lossy().to_fixed_bytes();
    let payload = vft_payload("Transfer", (recipient, MIRROR_BURST_TRANSFER_AMOUNT.to_string()));

    let send_futures = (0..MIRROR_BURST_SIZE).map(|index| {
        let provider = provider.clone();
        let payload = payload.clone();
        async move {
            IMirror::IMirrorInstance::new(mirror_address.into(), provider)
                .sendMessage(payload.into(), false)
                .value(AlloyU256::from(0))
                .nonce(base_account_nonce + index as u64)
                .send()
                .await
                .map_err(anyhow::Error::from)
        }
    });
    let pending_txs: Vec<_> = futures_util::future::try_join_all(send_futures).await?;

    let receipt_futures = pending_txs
        .into_iter()
        .map(|pending_tx| pending_tx.try_get_receipt_check_reverted());
    let receipts = futures_util::future::try_join_all(receipt_futures).await?;
    assert_eq!(receipts.len(), MIRROR_BURST_SIZE);
    for receipt in &receipts {
        assert!(receipt.status(), "expected successful burst receipt");
        assert!(
            receipt.block_hash.is_some(),
            "expected block hash in burst receipt"
        );
    }

    let message_ids = receipts
        .iter()
        .map(extract_message_id_from_receipt)
        .collect::<Result<Vec<_>>>()?;
    let unique_message_ids: HashSet<_> = message_ids.iter().copied().collect();
    assert_eq!(unique_message_ids.len(), MIRROR_BURST_SIZE);
    info!(
        receipts = receipts.len(),
        unique_message_ids = unique_message_ids.len(),
        "Collected burst message ids from receipts"
    );

    let reply_futures = message_ids
        .iter()
        .copied()
        .map(|message_id| mirror.wait_for_reply(message_id));
    let replies = futures_util::future::try_join_all(reply_futures).await?;
    assert_eq!(replies.len(), MIRROR_BURST_SIZE);
    for (index, reply) in replies.into_iter().enumerate() {
        let reply_code = reply.code.to_bytes();
        if reply_code != MANUAL_SUCCESS_REPLY_CODE {
            info!(
                reply_index = index,
                reply_code = ?reply_code,
                reply_reason = %describe_reply_code(&reply_code),
                "Burst transfer returned non-success reply"
            );
        }
        assert_manual_success(&reply_code, "mirror burst transfer");
        assert!(decode_sails_reply::<bool>(&reply.payload, "Vft", "Transfer")?);
    }

    log_step("Burst Final State");
    let expected_owner_balance = owner_balance_before - burst_total_amount;
    let expected_recipient_balance = recipient_balance_before + burst_total_amount;
    let expected_total_supply = total_supply_before;
    let final_state_hash = wait_for_expected_token_state(
        &ctx.api,
        ctx.program_id,
        ctx.owner_actor_id,
        recipient,
        burst_start_state_hash,
        expected_owner_balance,
        expected_recipient_balance,
        expected_total_supply,
    )
    .await?;
    let owner_balance_after = balance_of(&ctx.api, ctx.program_id, ctx.owner_actor_id).await?;
    let recipient_balance_after = balance_of(&ctx.api, ctx.program_id, recipient).await?;
    let total_supply_after = total_supply(&ctx.api, ctx.program_id).await?;
    let mirror_nonce_after = mirror.nonce().await?;

    assert_eq!(owner_balance_after, expected_owner_balance);
    assert_eq!(recipient_balance_after, expected_recipient_balance);
    assert_eq!(total_supply_after, expected_total_supply);
    assert_eq!(
        mirror_nonce_after - mirror_nonce_before,
        U256::from(MIRROR_BURST_SIZE as u64),
        "expected mirror nonce delta to equal burst size"
    );
    info!(
        %final_state_hash,
        base_account_nonce,
        owner_balance_after,
        recipient_balance_after,
        total_supply_after,
        mirror_nonce_before = %mirror_nonce_before,
        mirror_nonce_after = %mirror_nonce_after,
        "Mirror parallel burst reached expected final state"
    );

    Ok(())
}

#[tokio::test]
async fn vft_injected_parallel_burst_on_testnet() -> Result<()> {
    init_tracing();
    log_step("Setup");
    let ctx = setup_vft_program().await?;
    let mirror = ctx.api.mirror(ctx.program_id);
    let recipient = ActorId::from(88_u64);
    let burst_total_amount = INJECTED_BURST_SIZE as u128 * INJECTED_BURST_TRANSFER_AMOUNT;

    log_step("Initial Mint");
    let burst_start_state_hash = mirror_mint_with_state_change(&ctx, burst_total_amount).await?;
    let owner_balance_before = balance_of(&ctx.api, ctx.program_id, ctx.owner_actor_id).await?;
    let recipient_balance_before = balance_of(&ctx.api, ctx.program_id, recipient).await?;
    let total_supply_before = total_supply(&ctx.api, ctx.program_id).await?;
    let mirror_nonce_before = mirror.nonce().await?;

    assert_eq!(owner_balance_before, burst_total_amount);
    assert_eq!(recipient_balance_before, 0);
    assert_eq!(total_supply_before, burst_total_amount);

    log_step("Burst Top Up");
    info!(
        additional_top_up = BURST_TOP_UP_AMOUNT,
        "Adding extra executable balance for parallel injected burst"
    );
    top_up_executable_balance(&ctx.api, ctx.program_id, BURST_TOP_UP_AMOUNT).await?;
    let burst_executable_balance = executable_balance(&ctx.api, ctx.program_id).await?;
    info!(
        burst_executable_balance,
        "Executable balance before parallel injected burst"
    );

    log_step("Parallel Injected Burst");
    // This is a concurrent submission burst from the client side.
    // The SDK helper does not show explicit local serialization, but actual
    // execution ordering inside the node may still be partially sequential.
    let payload = vft_payload(
        "Transfer",
        (recipient, INJECTED_BURST_TRANSFER_AMOUNT.to_string()),
    );
    let injected_futures = (0..INJECTED_BURST_SIZE).map(|index| {
        let mirror = &mirror;
        let payload = payload.clone();
        async move {
            mirror
                .send_message_injected_and_watch(payload, 0)
                .await
                .with_context(|| format!("failed to send injected burst message at index {index}"))
        }
    });
    let injected_results = futures_util::future::try_join_all(injected_futures).await?;
    assert_eq!(injected_results.len(), INJECTED_BURST_SIZE);

    let unique_message_ids: HashSet<_> = injected_results
        .iter()
        .map(|(message_id, _)| *message_id)
        .collect();
    assert_eq!(unique_message_ids.len(), INJECTED_BURST_SIZE);
    info!(
        injected_promises = injected_results.len(),
        unique_message_ids = unique_message_ids.len(),
        "Collected injected burst promises"
    );

    for (index, (message_id, promise)) in injected_results.into_iter().enumerate() {
        if index > 0 && index % 25 == 0 {
            info!(
                processed = index,
                total = INJECTED_BURST_SIZE,
                "Processed injected burst replies"
            );
        }

        let reply_code = promise.reply.code.to_bytes();
        if reply_code != MANUAL_SUCCESS_REPLY_CODE {
            info!(
                reply_index = index,
                message_id = %message_id,
                reply_code = ?reply_code,
                reply_reason = %describe_reply_code(&reply_code),
                "Injected burst transfer returned non-success reply"
            );
        }
        info!(
            reply_index = index,
            message_id = %message_id,
            reply_code = ?reply_code,
            "Injected burst promise received"
        );
        assert_manual_success(&reply_code, "injected burst transfer");
        assert!(decode_sails_reply::<bool>(&promise.reply.payload, "Vft", "Transfer")?);
    }

    log_step("Burst Final State");
    let expected_owner_balance = owner_balance_before - burst_total_amount;
    let expected_recipient_balance = recipient_balance_before + burst_total_amount;
    let expected_total_supply = total_supply_before;
    let final_state_hash = wait_for_expected_token_state(
        &ctx.api,
        ctx.program_id,
        ctx.owner_actor_id,
        recipient,
        burst_start_state_hash,
        expected_owner_balance,
        expected_recipient_balance,
        expected_total_supply,
    )
    .await?;
    let owner_balance_after = balance_of(&ctx.api, ctx.program_id, ctx.owner_actor_id).await?;
    let recipient_balance_after = balance_of(&ctx.api, ctx.program_id, recipient).await?;
    let total_supply_after = total_supply(&ctx.api, ctx.program_id).await?;
    let mirror_nonce_after = mirror.nonce().await?;

    assert_eq!(owner_balance_after, expected_owner_balance);
    assert_eq!(recipient_balance_after, expected_recipient_balance);
    assert_eq!(total_supply_after, expected_total_supply);
    assert_eq!(
        mirror_nonce_after, mirror_nonce_before,
        "expected injected burst to leave mirror nonce unchanged"
    );
    info!(
        %final_state_hash,
        owner_balance_after,
        recipient_balance_after,
        total_supply_after,
        mirror_nonce_before = %mirror_nonce_before,
        mirror_nonce_after = %mirror_nonce_after,
        "Injected parallel burst reached expected final state"
    );

    Ok(())
}

#[tokio::test]
async fn vft_negative_cases_on_testnet() -> Result<()> {
    init_tracing();
    log_step("Setup");
    let ctx = setup_vft_program().await?;
    let mirror = ctx.api.mirror(ctx.program_id);
    let recipient = ActorId::from(99_u64);
    let payload = vft_payload("Transfer", (recipient, "1".to_string()));
    let supply_before = total_supply(&ctx.api, ctx.program_id).await?;

    log_step("Non-Zero Value Reject");
    let send_error = mirror
        .send_message_injected(payload.clone(), 1)
        .await
        .expect_err("expected injected send to reject non-zero value");
    let send_error_message = send_error.to_string();
    info!(error = %send_error_message, "Observed injected reject for send_message_injected");
    assert!(
        send_error_message.contains("non-zero value")
            || send_error_message.contains("non zero value"),
        "expected reject reason to mention unsupported non-zero value, got: {send_error_message}"
    );

    log_step("Non-Zero Value Watch Reject");
    let watch_error = mirror
        .send_message_injected_and_watch(payload.clone(), 1)
        .await
        .expect_err("expected injected watch to reject non-zero value");
    let watch_error_message = watch_error.to_string();
    info!(
        error = %watch_error_message,
        "Observed injected reject for send_message_injected_and_watch"
    );
    assert!(
        watch_error_message.contains("non-zero value")
            || watch_error_message.contains("non zero value"),
        "expected watch reject reason to mention unsupported non-zero value, got: {watch_error_message}"
    );

    log_step("Mirror Userspace Panic");
    let (panic_receipt, panic_message_id) = mirror
        .send_message_with_receipt(payload.clone(), 0)
        .await
        .with_context(|| "failed to send mirror userspace panic message")?;
    assert!(panic_receipt.status(), "expected successful mirror panic receipt");
    let panic_reply = mirror
        .wait_for_reply(panic_message_id)
        .await
        .with_context(|| "failed to wait for mirror userspace panic reply")?;
    let panic_code = panic_reply.code.to_bytes();
    info!(
        reply_code = ?panic_code,
        reply_reason = %describe_reply_code(&panic_code),
        "Observed mirror userspace panic"
    );
    assert_reply_code(&panic_code, USERSPACE_PANIC_REPLY_CODE, "mirror userspace panic");
    let supply_after_mirror_panic = total_supply(&ctx.api, ctx.program_id).await?;
    assert_eq!(supply_after_mirror_panic, supply_before);

    log_step("Injected Userspace Panic");
    let (_panic_message_id, panic_promise) = mirror
        .send_message_injected_and_watch(payload.clone(), 0)
        .await
        .with_context(|| "failed to send injected userspace panic message")?;
    let injected_panic_code = panic_promise.reply.code.to_bytes();
    info!(
        reply_code = ?injected_panic_code,
        reply_reason = %describe_reply_code(&injected_panic_code),
        "Observed injected userspace panic"
    );
    assert_reply_code(
        &injected_panic_code,
        USERSPACE_PANIC_REPLY_CODE,
        "injected userspace panic",
    );
    let supply_after_injected_panic = total_supply(&ctx.api, ctx.program_id).await?;
    assert_eq!(supply_after_injected_panic, supply_before);

    log_step("Uninitialized Program");
    let uninitialized_ctx = deploy_vft_program(false).await?;
    let uninitialized_mirror = uninitialized_ctx.api.mirror(uninitialized_ctx.program_id);
    let transfer_payload = vft_payload("Transfer", (recipient, "1".to_string()));

    let (uninit_receipt, uninit_message_id) = uninitialized_mirror
        .send_message_with_receipt(transfer_payload.clone(), 0)
        .await
        .with_context(|| "failed to send mirror message to uninitialized VFT")?;
    assert!(uninit_receipt.status(), "expected successful mirror receipt for uninitialized VFT");
    let uninit_reply = uninitialized_mirror
        .wait_for_reply(uninit_message_id)
        .await
        .with_context(|| "failed to wait for mirror uninitialized reply")?;
    let uninit_code = uninit_reply.code.to_bytes();
    info!(
        reply_code = ?uninit_code,
        reply_reason = %describe_reply_code(&uninit_code),
        "Observed mirror uninitialized reply"
    );
    // For the current VFT implementation, calling a method before init reaches
    // storage access that panics ("Storage is not initialized"), so the node
    // surfaces this as UserspacePanic instead of UnavailableActor::Uninitialized.
    assert_reply_code(
        &uninit_code,
        USERSPACE_PANIC_REPLY_CODE,
        "mirror uninitialized VFT userspace panic",
    );

    let (_uninit_injected_message_id, uninit_injected_promise) = uninitialized_mirror
        .send_message_injected_and_watch(transfer_payload, 0)
        .await
        .with_context(|| "failed to send injected message to uninitialized VFT")?;
    let uninit_injected_code = uninit_injected_promise.reply.code.to_bytes();
    info!(
        reply_code = ?uninit_injected_code,
        reply_reason = %describe_reply_code(&uninit_injected_code),
        "Observed injected uninitialized reply"
    );
    // Mirror canonical messages can reach the contract and trip its storage
    // access panic before init. Injected messages are filtered earlier by the
    // node, so the same destination surfaces as InitializationFailure here.
    assert_reply_code(
        &uninit_injected_code,
        INITIALIZATION_FAILURE_REPLY_CODE,
        "injected uninitialized VFT initialization failure",
    );

    Ok(())
}
