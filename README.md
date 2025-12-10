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

2. **Run the full workflow**

   This will:

   * deploy checker programs,
   * save their `programId`s,
   * deploy and initialize the manager,
   * register the checkers and start the computation.

   ```bash
   pnpm test:ui
   ```
    
    
## Parallel Mandelbrot Set Calculation Using Smart Contracts

The Mandelbrot set represents a classic example of computational complexity. Calculating this set often involves handling millions of data points and requires significant computational power. This example demonstrates how these computations can be performed using smart contracts on **gear.exe**.

### Mandelbrot Manager and Checker Smart Contracts

These smart contracts collaboratively calculate the Mandelbrot set by generating and evaluating points using distributed computation. The system comprises two contracts: **Manager** and **Checker**.

### Manager Contract
The Manager contract is responsible for orchestrating the computation. Its primary functions include:

1. **Point Generation**:
- Divides the complex plane into a grid of points based on user-defined parameters (e.g., resolution, bounds).
- Generates points and stores them along with their metadata.

2. **Task Distribution**:
- Distributes the generated points to multiple Checker contracts for computation.

3. **Result Aggregation**:
- Collects results from the Checker contracts to determine whether points belong to the Mandelbrot set.
- Updates the state for each processed point.

4. **Key Features**:
- **Parallelism**: Multiple Checker contracts work in parallel to compute the Mandelbrot set, demonstrating the power of distributed computation.
- **Continuous Execution with Reverse Gas Model**: Using the reverse gas model, the Manager contract can continuously compute the entire set of points after sending a single `generate_and_store_points` message with `check_points_after_generation = true`. The contract spends its own balance to fund this operation, ensuring uninterrupted execution.

### Checker Contract
The Checker contract evaluates whether points belong to the Mandelbrot set. Its primary functions include:

1. **Point Evaluation**:
- Accepts batches of points from the Manager contract.
- Iteratively computes the Mandelbrot escape condition for each point up to a maximum number of iterations.
2. **Result Reporting**:
- Returns the computation results (e.g., iteration counts) to the Manager contract.
3. **Computation Details**:
- Evaluates each point based on its coordinates in the complex plane and determines whether the point "escapes" or remains bounded.

### Workflow
1. The Manager generates a grid of complex points within user-defined bounds and parameters.
2. Points are distributed to Checker contracts for parallel evaluation.
3. Each Checker processes its batch of points and reports results back to the Manager.
4. The Manager collects and stores the results, marking points as either inside or outside the Mandelbrot set.
