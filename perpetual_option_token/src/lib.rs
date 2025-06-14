use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer};

declare_id!("8uXMvRdqojLoJx7DsYZeGzV3BkkaGCjrt3oUmD9UNpv9");

#[program]
pub mod perpetual_option_token {
    use super::*;

    /// Initialize global state: strike price, collateral ratio, paused flag,
    /// USDC vault, treasury vault, and pCALL mint.
    pub fn initialize(
        ctx: Context<Initialize>,
        strike_price: u64,            // e.g. 30_000_00000000 for $30k (8d)
        collateralization_ratio: u64, // e.g. 150_0000 for 150% (fixed-pt)
    ) -> Result<()> {
        let cfg = &mut ctx.accounts.config;
        cfg.authority = *ctx.accounts.authority.key;
        cfg.strike_price = strike_price;
        cfg.collateralization_ratio = collateralization_ratio;
        cfg.paused = false;
        Ok(())
    }

    /// Mint pCALL by depositing USDC. Enforces collateralization & mint fee.
    pub fn mint_option(ctx: Context<MintOption>, amount: u64) -> Result<()> {
        let cfg = &ctx.accounts.config;
        require!(!cfg.paused, OptionError::Paused);

        // collateralization check: deposit * 1e6 >= minted * ratio
        require!(
            amount
                .checked_mul(1_000_000)
                .ok_or(OptionError::Undercollateralized)?
                >= amount.checked_mul(cfg.collateralization_ratio).ok_or(OptionError::Undercollateralized)?,
            OptionError::Undercollateralized
        );

        // fee = 0.1%
        let fee = amount.checked_div(1_000).ok_or(OptionError::Undercollateralized)?;
        let net = amount.checked_sub(fee).ok_or(OptionError::Undercollateralized)?;

        // Get the seeds for signing
        let config_seeds = &[
            b"config".as_ref(),
            &[ctx.bumps.config]
        ];

        // split collateral: fee→treasury, net→vault
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_collateral.to_account_info(),
                    to: ctx.accounts.treasury_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            fee,
        )?;
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_collateral.to_account_info(),
                    to: ctx.accounts.vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            net,
        )?;

        // mint net pCALL to user
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.pcall_mint.to_account_info(),
                    to: ctx.accounts.user_pcall.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                &[config_seeds],
            ),
            net,
        )?;

        // update/create position
        let pos = &mut ctx.accounts.position;
        pos.owner = *ctx.accounts.user.key;
        pos.amount = pos.amount.checked_add(net).ok_or(OptionError::Undercollateralized)?;
        pos.timestamp = Clock::get()?.unix_timestamp;
        Ok(())
    }

    /// Redeem pCALL for (price – strike) × amount, minus a small fee.
    pub fn redeem_option(ctx: Context<RedeemOption>, amount: u64) -> Result<()> {
        let cfg = &ctx.accounts.config;
        require!(!cfg.paused, OptionError::Paused);

        // position expiry: 90 days
        let now = Clock::get()?.unix_timestamp;
        let elapsed = now.checked_sub(ctx.accounts.position.timestamp).ok_or(OptionError::ExpiredPosition)?;
        require!(elapsed < 90 * 86_400, OptionError::ExpiredPosition);

        // price check
        let oracle = &ctx.accounts.oracle;
        require!(oracle.price > cfg.strike_price, OptionError::BelowStrike);

        // compute payout
        let diff = oracle.price.checked_sub(cfg.strike_price).ok_or(OptionError::BelowStrike)?;
        let raw_payout = diff
            .checked_mul(amount)
            .ok_or(OptionError::Undercollateralized)?
            .checked_div(10u64.pow(8))
            .ok_or(OptionError::Undercollateralized)?;

        // fee = 0.1%
        let fee = raw_payout.checked_div(1_000).ok_or(OptionError::Undercollateralized)?;
        let net = raw_payout.checked_sub(fee).ok_or(OptionError::Undercollateralized)?;

        // Get the seeds for signing
        let config_seeds = &[
            b"config".as_ref(),
            &[ctx.bumps.config]
        ];

        // burn pCALL
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.pcall_mint.to_account_info(),
                    from: ctx.accounts.user_pcall.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount,
        )?;

        // pay user, send fee to treasury
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.user_collateral.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                &[config_seeds],
            ),
            net,
        )?;
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.treasury_vault.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                &[config_seeds],
            ),
            fee,
        )?;

        // update position
        let pos = &mut ctx.accounts.position;
        pos.amount = pos.amount.checked_sub(amount).ok_or(OptionError::Undercollateralized)?;
        Ok(())
    }

    /// Admin: update the strike price on-chain.
    pub fn update_strike_price(ctx: Context<AdminUpdate>, new_price: u64) -> Result<()> {
        let cfg = &mut ctx.accounts.config;
        cfg.strike_price = new_price;
        Ok(())
    }

    /// Admin: pause or unpause the protocol.
    pub fn set_paused(ctx: Context<AdminUpdate>, paused: bool) -> Result<()> {
        let cfg = &mut ctx.accounts.config;
        cfg.paused = paused;
        Ok(())
    }

    /// Liquidate undercollateralized positions.
    pub fn liquidate(ctx: Context<Liquidate>) -> Result<()> {
        let vault_balance = ctx.accounts.vault.amount;
        let cfg = &ctx.accounts.config;
        let oracle = &ctx.accounts.oracle;
        let pos_amount = ctx.accounts.position.amount;

        let diff = oracle.price.checked_sub(cfg.strike_price).ok_or(OptionError::BelowStrike)?;
        let due = diff
            .checked_mul(pos_amount)
            .ok_or(OptionError::Undercollateralized)?
            .checked_div(10u64.pow(8))
            .ok_or(OptionError::Undercollateralized)?;

        require!(vault_balance < due, OptionError::Undercollateralized);

        // Get the seeds for signing
        let config_seeds = &[
            b"config".as_ref(),
            &[ctx.bumps.config]
        ];

        // transfer entire vault to liquidator
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.liquidator_collateral.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                &[config_seeds],
            ),
            vault_balance,
        )?;

        ctx.accounts.position.amount = 0;
        Ok(())
    }

    /// View-only: compute payout for a given amount.
    pub fn get_payout(ctx: Context<GetPayout>, amount: u64) -> Result<u64> {
        let cfg = &ctx.accounts.config;
        let oracle = &ctx.accounts.oracle;
        
        if oracle.price <= cfg.strike_price {
            return Ok(0);
        }
        
        let diff = oracle.price.checked_sub(cfg.strike_price).ok_or(OptionError::BelowStrike)?;
        let payout = diff
            .checked_mul(amount)
            .ok_or(OptionError::Undercollateralized)?
            .checked_div(10u64.pow(8))
            .ok_or(OptionError::Undercollateralized)?;
        
        Ok(payout)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTEXTS & CPI HELPERS
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut, signer)]
    pub authority: AccountInfo<'info>,

    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 8 + 8 + 1,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,

    #[account(
        init,
        payer = authority,
        mint::decimals = 8,
        mint::authority = config,
        seeds = [b"pcall_mint"],
        bump
    )]
    pub pcall_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        token::mint = usdc_mint,
        token::authority = config,
        seeds = [b"vault"],
        bump
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        token::mint = usdc_mint,
        token::authority = config,
        seeds = [b"treasury_vault"],
        bump
    )]
    pub treasury_vault: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct MintOption<'info> {
    #[account(mut, signer)]
    pub user: AccountInfo<'info>,

    #[account(mut)]
    pub user_collateral: Account<'info, TokenAccount>,

    #[account(mut, seeds = [b"vault"], bump)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut, seeds = [b"treasury_vault"], bump)]
    pub treasury_vault: Account<'info, TokenAccount>,

    #[account(mut, seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [b"pcall_mint"], bump)]
    pub pcall_mint: Account<'info, Mint>,

    #[account(mut)]
    pub user_pcall: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        space = 8 + 32 + 8 + 8,
        seeds = [b"position", user.key().as_ref()],
        bump
    )]
    pub position: Account<'info, Position>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

impl<'info> MintOption<'info> {
    // Helper methods removed - using inline CPI contexts instead
}

#[derive(Accounts)]
pub struct RedeemOption<'info> {
    #[account(signer)]
    pub user: AccountInfo<'info>,

    #[account(mut)]
    pub user_pcall: Account<'info, TokenAccount>,
    #[account(mut, seeds = [b"pcall_mint"], bump)]
    pub pcall_mint: Account<'info, Mint>,

    #[account(mut, seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [b"vault"], bump)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut, seeds = [b"treasury_vault"], bump)]
    pub treasury_vault: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_collateral: Account<'info, TokenAccount>,

    #[account(mut, seeds = [b"position", user.key().as_ref()], bump)]
    pub position: Account<'info, Position>,

    #[account(mut, seeds = [b"oracle"], bump)]
    pub oracle: Account<'info, PriceOracle>,

    pub token_program: Program<'info, Token>,
}

impl<'info> RedeemOption<'info> {
    // Helper methods removed - using inline CPI contexts instead
}

#[derive(Accounts)]
pub struct AdminUpdate<'info> {
    #[account(signer)]
    pub authority: AccountInfo<'info>,
    #[account(mut, seeds = [b"config"], bump, has_one = authority)]
    pub config: Account<'info, Config>,
}

#[derive(Accounts)]
pub struct Liquidate<'info> {
    #[account(signer)]
    pub liquidator: AccountInfo<'info>,

    #[account(mut, seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,

    #[account(mut, seeds = [b"vault"], bump)]
    pub vault: Account<'info, TokenAccount>,

    #[account(mut, seeds = [b"treasury_vault"], bump)]
    pub treasury_vault: Account<'info, TokenAccount>,

    #[account(mut, seeds = [b"position", liquidator.key().as_ref()], bump)]
    pub position: Account<'info, Position>,

    #[account(mut, seeds = [b"oracle"], bump)]
    pub oracle: Account<'info, PriceOracle>,

    #[account(mut)]
    pub liquidator_collateral: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

impl<'info> Liquidate<'info> {
    // Helper methods removed - using inline CPI contexts instead
}

#[derive(Accounts)]
pub struct GetPayout<'info> {
    #[account(seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,
    #[account(seeds = [b"oracle"], bump)]
    pub oracle: Account<'info, PriceOracle>,
}

// ─────────────────────────────────────────────────────────────────────────────
// ACCOUNTS
// ─────────────────────────────────────────────────────────────────────────────

#[account]
pub struct Config {
    pub authority: Pubkey,
    pub strike_price: u64,
    pub collateralization_ratio: u64,
    pub paused: bool,
}

impl Config {
    pub fn seeds(&self) -> Vec<Vec<u8>> {
        vec![
            b"config".to_vec(),
            vec![Pubkey::find_program_address(&[b"config"], &crate::id()).1]
        ]
    }
}

#[account]
pub struct Position {
    pub owner: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[account]
pub struct PriceOracle {
    pub price: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// ERRORS
// ─────────────────────────────────────────────────────────────────────────────

#[error_code]
pub enum OptionError {
    #[msg("Current price is below strike.")]
    BelowStrike,
    #[msg("Position undercollateralized.")]
    Undercollateralized,
    #[msg("Protocol is paused.")]
    Paused,
    #[msg("Position has expired.")]
    ExpiredPosition,
}