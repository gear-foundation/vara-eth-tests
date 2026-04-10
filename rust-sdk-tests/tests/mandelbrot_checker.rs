use anyhow::{Context, Result};
use parity_scale_codec::Encode;
use rust_sdk_tests::{
    config::TestConfig,
    test_helpers::{
        assert_auto_success, init_tracing, log_step, top_up_executable_balance,
        wait_for_program_on_vara_eth, wait_for_state_hash_change,
    },
};
use tracing::info;

use ethexe_sdk::VaraEthApi;
use gprimitives::{ActorId, CodeId, H256};
use mandelbrot_checker_app::{FixedPoint, Point};

const CHECKER_TOP_UP_AMOUNT: u128 = 100 * 1_000_000_000_000;
const BATCH_SIZES: &[usize] = &[10, 20, 50, 100];
const MAX_ITER: u32 = 1_000;
const CHECKER_POINT_RE_NUM: i64 = -776;
const CHECKER_POINT_RE_SCALE: u32 = 3;
const CHECKER_POINT_IM_NUM: i64 = 113;
const CHECKER_POINT_IM_SCALE: u32 = 3;
const VARA_DECIMALS: u128 = 1_000_000_000_000;

struct CheckerContext {
    api: VaraEthApi,
    program_id: ActorId,
}

#[derive(Debug)]
struct MirrorBatchMetrics {
    points: usize,
    executable_balance_before: u128,
    executable_balance_after: u128,
    executable_balance_delta: u128,
}

#[derive(Debug)]
struct InjectedBatchMetrics {
    points: usize,
    executable_balance_before: u128,
    executable_balance_after: u128,
    executable_balance_delta: u128,
}

async fn setup_checker_program() -> Result<CheckerContext> {
    let config = TestConfig::load()?;
    let api = config.connect_api().await?;
    let router = api.router();
    let code_id: CodeId = config.checker_code_id()?;

    let (receipt, program_id) = router
        .create_program_with_receipt(code_id, H256::random(), None)
        .await
        .with_context(|| "failed to create mandelbrot-checker program")?;

    assert!(
        receipt.status(),
        "expected successful createProgram receipt for mandelbrot-checker"
    );
    assert!(
        receipt.block_hash.is_some(),
        "expected block hash in createProgram receipt for mandelbrot-checker"
    );

    wait_for_program_on_vara_eth(&api, program_id).await?;
    top_up_executable_balance(&api, program_id, CHECKER_TOP_UP_AMOUNT).await?;

    let mirror = api.mirror(program_id);
    let previous_state_hash = mirror.state_hash().await?;
    let (init_receipt, init_message_id) = mirror
        .send_message_with_receipt(init_payload(), 0)
        .await
        .with_context(|| "failed to send mandelbrot-checker init message")?;
    assert!(
        init_receipt.status(),
        "expected successful init receipt for mandelbrot-checker"
    );
    let init_reply = mirror
        .wait_for_reply(init_message_id)
        .await
        .with_context(|| "failed to wait for mandelbrot-checker init reply")?;
    assert_auto_success(&init_reply.code.to_bytes(), "mandelbrot-checker init");
    let state_hash_after_init = wait_for_state_hash_change(&api, program_id, previous_state_hash)
        .await
        .with_context(|| "failed to observe state hash change after mandelbrot-checker init")?;

    info!(
        program_id = %program_id,
        %state_hash_after_init,
        "Mandelbrot-checker program initialized"
    );

    Ok(CheckerContext { api, program_id })
}

fn init_payload() -> Vec<u8> {
    ["Init".to_string().encode(), ().encode()].concat()
}

fn checker_payload(points_count: usize, max_iter: u32) -> Vec<u8> {
    let points = sample_points(points_count);
    let points_u16 = encode_points_as_u16(&points);

    [
        "MandelbrotChecker".to_string().encode(),
        "CheckMandelbrotPoints".to_string().encode(),
        (points_u16, max_iter).encode(),
    ]
    .concat()
}

fn encode_points_as_u16(points: &[Point]) -> Vec<u16> {
    points.encode().into_iter().map(u16::from).collect()
}

fn sample_points(count: usize) -> Vec<Point> {
    (0..count)
        .map(|index| Point {
            index: index as u32,
            c_re: FixedPoint {
                num: CHECKER_POINT_RE_NUM,
                scale: CHECKER_POINT_RE_SCALE,
            },
            c_im: FixedPoint {
                num: CHECKER_POINT_IM_NUM,
                scale: CHECKER_POINT_IM_SCALE,
            },
        })
        .collect()
}

fn format_vara(amount: u128) -> String {
    let whole = amount / VARA_DECIMALS;
    let frac = amount % VARA_DECIMALS;
    format!("{whole}.{frac:012}")
}

async fn run_mirror_batch(
    ctx: &CheckerContext,
    points: usize,
) -> Result<MirrorBatchMetrics> {
    let mirror = ctx.api.mirror(ctx.program_id);
    let payload = checker_payload(points, MAX_ITER);
    let nonce_before = mirror.nonce().await?;
    let state_hash_before = mirror.state_hash().await?;
    let executable_balance_before = mirror.state().await?.executable_balance;

    let (receipt, message_id) = mirror
        .send_message_with_receipt(payload, 0)
        .await
        .with_context(|| format!("failed to send mirror mandelbrot batch with {points} points"))?;

    assert!(
        receipt.status(),
        "expected successful mirror receipt for mandelbrot batch of {points} points"
    );

    let reply = mirror
        .wait_for_reply(message_id)
        .await
        .with_context(|| format!("failed to wait for mirror reply for {points} points"))?;

    assert_auto_success(&reply.code.to_bytes(), "mandelbrot mirror batch");
    let state_hash_after =
        wait_for_state_hash_change(&ctx.api, ctx.program_id, state_hash_before).await?;
    let nonce_after = mirror.nonce().await?;
    let executable_balance_after = mirror.state().await?.executable_balance;
    assert!(
        nonce_after > nonce_before,
        "expected mirror nonce to increase after mandelbrot mirror batch"
    );

    let executable_balance_delta =
        executable_balance_before.saturating_sub(executable_balance_after);
    info!(
        points,
        max_iter = MAX_ITER,
        point_re_num = CHECKER_POINT_RE_NUM,
        point_re_scale = CHECKER_POINT_RE_SCALE,
        point_im_num = CHECKER_POINT_IM_NUM,
        point_im_scale = CHECKER_POINT_IM_SCALE,
        message_id = %message_id,
        nonce_before = %nonce_before,
        nonce_after = %nonce_after,
        state_hash_before = %state_hash_before,
        state_hash_after = %state_hash_after,
        executable_balance_before_vara = %format_vara(executable_balance_before),
        executable_balance_after_vara = %format_vara(executable_balance_after),
        executable_balance_delta_vara = %format_vara(executable_balance_delta),
        executable_balance_delta_per_point_vara =
            %format_vara(executable_balance_delta / points as u128),
        "Mirror batch completed"
    );

    Ok(MirrorBatchMetrics {
        points,
        executable_balance_before,
        executable_balance_after,
        executable_balance_delta,
    })
}

async fn run_injected_batch(
    ctx: &CheckerContext,
    points: usize,
) -> Result<InjectedBatchMetrics> {
    let mirror = ctx.api.mirror(ctx.program_id);
    let payload = checker_payload(points, MAX_ITER);
    let nonce_before = mirror.nonce().await?;
    let state_hash_before = mirror.state_hash().await?;
    let executable_balance_before = mirror.state().await?.executable_balance;

    let (message_id, promise) = mirror
        .send_message_injected_and_watch(payload, 0)
        .await
        .with_context(|| format!("failed to send injected mandelbrot batch with {points} points"))?;

    assert_auto_success(&promise.reply.code.to_bytes(), "mandelbrot injected batch");
    let state_hash_after =
        wait_for_state_hash_change(&ctx.api, ctx.program_id, state_hash_before).await?;
    let nonce_after = mirror.nonce().await?;
    let executable_balance_after = mirror.state().await?.executable_balance;
    let executable_balance_delta =
        executable_balance_before.saturating_sub(executable_balance_after);
    assert_eq!(
        nonce_after, nonce_before,
        "expected injected mandelbrot batch to leave mirror nonce unchanged"
    );

    info!(
        points,
        max_iter = MAX_ITER,
        point_re_num = CHECKER_POINT_RE_NUM,
        point_re_scale = CHECKER_POINT_RE_SCALE,
        point_im_num = CHECKER_POINT_IM_NUM,
        point_im_scale = CHECKER_POINT_IM_SCALE,
        message_id = %message_id,
        nonce_before = %nonce_before,
        nonce_after = %nonce_after,
        state_hash_before = %state_hash_before,
        state_hash_after = %state_hash_after,
        executable_balance_before_vara = %format_vara(executable_balance_before),
        executable_balance_after_vara = %format_vara(executable_balance_after),
        executable_balance_delta_vara = %format_vara(executable_balance_delta),
        executable_balance_delta_per_point_vara =
            %format_vara(executable_balance_delta / points as u128),
        "Injected batch completed"
    );

    Ok(InjectedBatchMetrics {
        points,
        executable_balance_before,
        executable_balance_after,
        executable_balance_delta,
    })
}

fn log_mirror_summary(metrics: &[MirrorBatchMetrics]) {
    for metric in metrics {
        info!(
            points = metric.points,
            executable_balance_before_vara = %format_vara(metric.executable_balance_before),
            executable_balance_after_vara = %format_vara(metric.executable_balance_after),
            executable_balance_delta_vara = %format_vara(metric.executable_balance_delta),
            executable_balance_delta_per_point_vara =
                %format_vara(metric.executable_balance_delta / metric.points as u128),
            "Mirror batch summary"
        );
    }

    for window in metrics.windows(2) {
        let previous = &window[0];
        let current = &window[1];
        info!(
            from_points = previous.points,
            to_points = current.points,
            executable_balance_delta_change_vara = %format_vara(
                current
                    .executable_balance_delta
                    .saturating_sub(previous.executable_balance_delta),
            ),
            "Mirror batch delta"
        );
    }
}

fn log_injected_summary(metrics: &[InjectedBatchMetrics]) {
    for metric in metrics {
        info!(
            points = metric.points,
            executable_balance_before_vara = %format_vara(metric.executable_balance_before),
            executable_balance_after_vara = %format_vara(metric.executable_balance_after),
            executable_balance_delta_vara = %format_vara(metric.executable_balance_delta),
            executable_balance_delta_per_point_vara =
                %format_vara(metric.executable_balance_delta / metric.points as u128),
            "Injected batch summary"
        );
    }
}

#[tokio::test]
async fn mandelbrot_checker_profile_on_testnet() -> Result<()> {
    init_tracing();

    log_step("Setup");
    let ctx = setup_checker_program().await?;
    let mirror = ctx.api.mirror(ctx.program_id);

    log_step("Mirror Batch Profile");
    let mut mirror_metrics = Vec::new();
    for &points in BATCH_SIZES {
        mirror_metrics.push(run_mirror_batch(&ctx, points).await?);
    }
    log_mirror_summary(&mirror_metrics);

    log_step("Injected Batch Profile");
    let mut injected_metrics = Vec::new();
    for &points in BATCH_SIZES {
        injected_metrics.push(run_injected_batch(&ctx, points).await?);
    }
    log_injected_summary(&injected_metrics);

    log_step("Final Readability");
    let state = mirror.state().await?;
    let full_state = mirror.full_state().await?;
    assert_eq!(state.balance, full_state.balance);
    assert_eq!(state.executable_balance, full_state.executable_balance);
    assert!(
        state.executable_balance > 0,
        "expected executable balance to remain positive after mandelbrot profiling scenario"
    );

    info!(
        program_id = %ctx.program_id,
        executable_balance = state.executable_balance,
        balance = state.balance,
        "Mandelbrot-checker remained readable after profiling scenario"
    );

    Ok(())
}
