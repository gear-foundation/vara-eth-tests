use anyhow::{Context, Result, anyhow};
use ethexe_ethereum::{Ethereum, EthereumBuilder};
use ethexe_sdk::VaraEthApi;
use gprimitives::ActorId;
use gsigner::secp256k1::{Address, PrivateKey, Signer};
use std::{collections::HashMap, env, fs, path::Path, str::FromStr};

#[derive(Debug, Clone)]
pub struct TestConfig {
    pub ethereum_rpc: String,
    pub vara_eth_rpc: String,
    pub router_address: String,
    pub private_key: String,
    pub checker_code_id: Option<String>,
    pub manager_code_id: Option<String>,
    pub token_id: Option<String>,
}

impl TestConfig {
    pub fn load() -> Result<Self> {
        let env_file = load_env_file("../.env")?;

        Ok(Self {
            ethereum_rpc: get_var("ETHEREUM_RPC", &env_file)?,
            vara_eth_rpc: get_var("VARA_ETH_RPC", &env_file)?,
            router_address: get_var("ROUTER_ADDRESS", &env_file)?,
            private_key: get_var("PRIVATE_KEY", &env_file)?,
            checker_code_id: get_optional_var("CHECKER_CODE_ID", &env_file),
            manager_code_id: get_optional_var("MANAGER_CODE_ID", &env_file),
            token_id: get_optional_var("TOKEN_ID", &env_file),
        })
    }

    pub fn checker_code_id(&self) -> Result<gprimitives::CodeId> {
        let checker_code_id = self
            .checker_code_id
            .as_deref()
            .ok_or_else(|| anyhow!("missing required config value: CHECKER_CODE_ID"))?;
        gprimitives::CodeId::from_str(checker_code_id)
            .with_context(|| format!("failed to parse CHECKER_CODE_ID: {checker_code_id}"))
    }

    pub fn manager_code_id(&self) -> Result<gprimitives::CodeId> {
        let manager_code_id = self
            .manager_code_id
            .as_deref()
            .ok_or_else(|| anyhow!("missing required config value: MANAGER_CODE_ID"))?;
        gprimitives::CodeId::from_str(manager_code_id)
            .with_context(|| format!("failed to parse MANAGER_CODE_ID: {manager_code_id}"))
    }

    pub fn token_actor_id(&self) -> Result<ActorId> {
        let token_id = self
            .token_id
            .as_deref()
            .ok_or_else(|| anyhow!("missing required config value: TOKEN_ID"))?;
        ActorId::from_str(token_id).with_context(|| format!("failed to parse TOKEN_ID: {token_id}"))
    }

    pub fn router_address(&self) -> Result<Address> {
        Address::from_str(&self.router_address)
            .with_context(|| format!("failed to parse ROUTER_ADDRESS: {}", self.router_address))
    }

    pub fn signer_and_address(&self) -> Result<(Signer, Address)> {
        let private_key = PrivateKey::from_str(self.private_key.trim_start_matches("0x"))
            .with_context(|| "failed to parse PRIVATE_KEY")?;
        let signer = Signer::memory();
        let public_key = signer
            .import(private_key)
            .with_context(|| "failed to import PRIVATE_KEY into signer")?;
        let address = public_key.to_address();

        Ok((signer, address))
    }

    pub async fn connect_ethereum(&self) -> Result<Ethereum> {
        let (signer, sender_address) = self.signer_and_address()?;
        EthereumBuilder::default()
            .rpc_url(&self.ethereum_rpc)
            .router_address(self.router_address()?)
            .signer(signer)
            .sender_address(sender_address)
            .eip1559_fee_increase_percentage(Ethereum::NO_EIP1559_FEE_INCREASE_PERCENTAGE)
            .blob_gas_multiplier(Ethereum::INCREASED_BLOB_GAS_MULTIPLIER)
            .build()
            .await
            .with_context(|| "failed to connect Ethereum client")
    }

    pub async fn connect_api(&self) -> Result<VaraEthApi> {
        let ethereum = self.connect_ethereum().await?;

        VaraEthApi::new(&self.vara_eth_rpc, ethereum)
            .await
            .with_context(|| "failed to connect Vara.Eth API client")
    }
}

fn get_var(key: &str, env_file: &HashMap<String, String>) -> Result<String> {
    get_optional_var(key, env_file)
        .ok_or_else(|| anyhow!("missing required config value: {key}"))
}

fn get_optional_var(key: &str, env_file: &HashMap<String, String>) -> Option<String> {
    env::var(key)
        .ok()
        .or_else(|| env_file.get(key).cloned())
        .filter(|value| !value.trim().is_empty())
}

fn load_env_file(path: impl AsRef<Path>) -> Result<HashMap<String, String>> {
    let path = path.as_ref();

    if !path.exists() {
        return Ok(HashMap::new());
    }

    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;

    let mut values = HashMap::new();

    for line in contents.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };

        let value = raw_value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();

        values.insert(key.trim().to_string(), value);
    }

    Ok(values)
}
