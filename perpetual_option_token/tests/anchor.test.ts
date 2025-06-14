
import { createMint, TOKEN_PROGRAM_ID } from "@solana/spl-token";

describe("PerpetualOptionToken", () => {
  it("should initialize the config, pcall mint, and vaults", async () => {
    const strikePrice = new BN(30_000_00000000);
    const collateralRatio = new BN(150_0000);

    // Create a real signer for payer/mint authority
    const payer = web3.Keypair.generate();

    // Airdrop SOL to payer
    const sig = await pg.connection.requestAirdrop(payer.publicKey, web3.LAMPORTS_PER_SOL);
    await pg.connection.confirmTransaction(sig);

    // Derive PDAs
    const [configPda] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("config")],
      pg.program.programId
    );
    const [vaultPda] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault")],
      pg.program.programId
    );
    const [treasuryVaultPda] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("treasury_vault")],
      pg.program.programId
    );
    const [pcallMintPda] = await web3.PublicKey.findProgramAddressSync(
      [Buffer.from("pcall_mint")],
      pg.program.programId
    );

    // Create USDC mint with payer and itself as mint authority
    const usdcMint = await createMint(
      pg.connection,
      payer,              // payer must be real Signer
      payer.publicKey,    // mint authority
      null,               // freeze authority
      8                   // decimals
    );

    // Send the initialize tx
    const tx = await pg.program.methods
      .initialize(strikePrice, collateralRatio)
      .accounts({
        authority: pg.wallet.publicKey,
        config: configPda,
        pcallMint: pcallMintPda,
        vault: vaultPda,
        treasuryVault: treasuryVaultPda,
        usdcMint: usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: web3.SystemProgram.programId,
        rent: web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([payer]) // we include the signer here for fee paying
      .rpc();

    console.log("✅ Initialize TX:", tx);

    // Validate state
    const config = await pg.program.account.config.fetch(configPda);
    console.log("Strike:", config.strikePrice.toString());
    console.log("Ratio:", config.collateralizationRatio.toString());
    console.log("Paused:", config.paused);

    assert(config.strikePrice.eq(strikePrice));
    assert(config.collateralizationRatio.eq(collateralRatio));
    assert.equal(config.paused, false);
  });
});
