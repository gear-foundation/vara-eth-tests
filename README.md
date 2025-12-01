1. **Build the smart contracts**

   From the repo root:

   ```bash
   cd contracts
   cargo build --release
   ```

2. **Install script dependencies**

   ```bash
   cd ../script
   yarn
   ```

3. **Run the full workflow**

   This will:

   * deploy checker programs,
   * save their `programId`s,
   * deploy and initialize the manager,
   * register the checkers and start the computation.

   ```bash
   yarn workflow:full
   ```

4. **Run parts separately**

   If the checker programs are already deployed, you don’t need to deploy them again — you can run only the manager part that performs the point computations.
Because of that, you can run the workflow in separate steps:
- Only deploy checker programs (e.g. first time, or when you want a fresh set):
    ```bash
    yarn create:checkers
    ```
- Only run the manager (using an existing list of checker programs):
    ```bash
    yarn create:manager
    ```