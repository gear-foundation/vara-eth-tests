use anyhow::Result;
use rust_sdk_tests::config::TestConfig;

#[tokio::test]
async fn router_exposes_core_addresses_and_token_program() -> Result<()> {
    let config = TestConfig::load()?;
    let api = config.connect_api().await?;
    let router = api.router();
    let token_actor_id = config.token_actor_id()?;

    let wvara_address = router.wvara_address().await?;
    let programs_count = router.programs_count().await?;
    let program_ids = router.program_ids().await?;
    println!("program_ids: {:?}", program_ids);

    assert_ne!(wvara_address.to_string(), "0x0000000000000000000000000000000000000000");
    assert!(programs_count > 0, "expected at least one program on testnet");
    assert!(
        program_ids.contains(&token_actor_id),
        "expected TOKEN_ID from config to appear in Vara-side program ids",
    );

    Ok(())
}

#[tokio::test]
async fn token_mirror_is_queryable_on_testnet() -> Result<()> {
    let config = TestConfig::load()?;
    let api = config.connect_api().await?;
    let mirror = api.mirror(config.token_actor_id()?);

    let state_hash = mirror.state_hash().await?;
    let code_id = mirror.code_id().await?;
    let state = mirror.state().await?;

    assert_ne!(state_hash.to_string(), "0x0000000000000000000000000000000000000000000000000000000000000000");
    assert_ne!(code_id.to_string(), "0x0000000000000000000000000000000000000000000000000000000000000000");
    assert!(!state.is_zero(), "expected TOKEN_ID mirror state to be non-zero");

    Ok(())
}
