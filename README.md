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

3. **Run the full workflow (recommended)**

   This will:

   * deploy checker programs,
   * save their `programId`s,
   * deploy and initialize the manager,
   * register the checkers and start the computation.

   ```bash
   yarn workflow:full
   ```

4. **(Optional) Run parts separately**

   If you want more control:

   * Only deploy checker programs:

     ```bash
     yarn create:checkers
     ```

   * Only deploy and run the manager (using existing checker list):

     ```bash
     yarn create:manager
     ```
