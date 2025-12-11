## Test coverage

The current end-to-end tests cover the full lifecycle of the VFT program on Vara-Eth.

### Deployment & funding (`create token` suite)

- ❌ Uploads the VFT code to Vara-Eth.
- ✅ Verifies that the final `codeId` is well-formed.
- ✅ Creates a new VFT program from the uploaded `codeId`.
- ✅ Waits until the program appears on Vara-Eth.
- ✅ Approves wVARA for the VFT program on Ethereum.
- ✅  Tops up the program executable balance.
- ✅  Reads program state from Vara-Eth and checks that `executableBalance == TOP_UP_AMOUNT`.
- ✅  Parses the generated IDL (`extended_vft.idl`).
- ✅  Sends the init message and checks:
  - transaction status is `success`,
  - reply code is `0x00000000`.

### Metadata queries (`metadata` suite)

- ✅  Reads the token `name` via a handle query and verifies it equals `"Name"`.
- ✅  Reads the token `symbol` and verifies it equals `"Symbol"`.
- ✅  Reads the token `decimals` and verifies it equals `12`.

### Messages through mirror: mint (`send messages: mint` suite)

- ✅  Sends a `Mint` message through mirror:
  - checks that the transaction succeeds on Ethereum,
  - checks reply code is `0x00010000`,
  - decodes the result and verifies it is `true`.
- ✅ Queries `BalanceOf(address)` and verifies that the balance equals the minted amount.
- ✅ Verifies that the program executable balance decreases after calls
      by comparing `executableBalance` before and after.

### Injected transactions: transfer (`injected txs: transfer` suite)
- ❌ Sends an injected `Transfer`:
  - sends the injected transaction and checks that `send()` returns `"Accept"`,
  - waits for the promise from `sendAndWaitForPromise()` to resolve.
- ❌ Queries `BalanceOf(recipient)` and verifies that the balance equals the transferred amount.

### Injected transactions: mint (`injected txs: mint` suite)

- ❌ Sends an injected `Mint` transaction:
  - sends the injected transaction and checks that `send()` returns `"Accept"`,
  - waits for the promise from `sendAndWaitForPromise()` to resolve.
- ❌ Queries `BalanceOf(recipient)` and verifies that the balance equals the minted amount.

## Setup Instructions
### 1. Environment configuration

1. Create your `.env` from the example

   In the project root, copy the example file:

   ```bash
   cp .env.example .env
   ```
2. Set your private key

   Open the new .env file and set your own Ethereum private key:
   
   ```env
   PRIVATE_KEY=0xYOUR_PRIVATE_KEY_HERE
   ```
3. Check RPC endpoints and contract IDs

   The defaults in .env.example are configured for:

   - ETHEREUM_RPC – public Hoodi Ethereum RPC.

   - VARA_ETH_RPC – public Vara-Eth validator node.

   - ROUTER_ADDRESS – router contract address on Ethereum.

   - CHECKER_CODE_ID, MANAGER_CODE_ID, TOKEN_ID – deployed code IDs on Vara-Eth.

   If you redeploy contracts, update these IDs accordingly.

   Note: the tests also upload the contract code.  
   If code upload does not succeed, the tests fall back to using
   the `CHECKER_CODE_ID`, `MANAGER_CODE_ID` and `TOKEN_ID` values from your `.env` file.

4. Account funding

   Before running the scripts, make sure that the account corresponding to your PRIVATE_KEY has:

   - sufficient ETH on the Hoodi Ethereum endpoint.

   - sufficient wrapped VARA on Vara-Eth.

### 2. Setup NVM and Node.js LTS

For detailed NVM setup instructions, see the [NVM documentation](https://github.com/nvm-sh/nvm).

Install Node.js LTS:

```bash
nvm install --lts
nvm use --lts
```

To verify Node.js is properly installed:
```bash
node -v   # Should show v20.x.x or later LTS version
npm -v    # Should show npm version
```

### 3. Setup pnpm

For detailed pnpm setup instructions, see the [pnpm documentation](https://pnpm.io/installation).

Install pnpm globally:

```bash
npm install -g pnpm
```

To verify pnpm is installed:
```bash
pnpm -v   # Should show version 10.x.x or later
```

### 4. Install Dependencies

Install all project dependencies:

```bash
pnpm install
```

This command will install all packages listed in `package.json` and create the `node_modules` directory.


1. **Build the smart contracts**

   From the repo root:

   ```bash
   cargo build --release
   ```

2. **Run the tests**

   ```bash
   pnpm test:ui
   ```
    
 