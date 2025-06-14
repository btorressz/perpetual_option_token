# perpetual_option_token

# Perpetual Option Token (POT)

**POT (Perpetual Option Token)** is a decentralized options protocol built on the Solana blockchain using the Anchor framework. It enables users to mint and redeem perpetual call options (pCALL) backed by USDC collateral, offering a programmable way to access leveraged upside exposure. This project was developed in Solana playground ide

---

## Overview

The POT smart contract allows users to mint pCALL tokens by depositing USDC and redeem them later for profits if the price of the underlying asset rises above the strike price. The protocol enforces a minimum collateralization ratio and applies minting and redemption fees, which are sent to a treasury vault. The system also includes liquidation logic for undercollateralized positions and administrative functions for protocol governance.

---

## Key Features

### Initialization

An authority initializes the protocol by setting the strike price, collateralization ratio, and creating essential on-chain accounts such as the USDC vault, treasury vault, and pCALL mint. This sets up the global configuration for the entire system.

### Minting pCALL Tokens

Users can mint pCALL options by depositing USDC. The amount minted is determined by the collateralization ratio. A small fee is taken from the deposit and routed to the treasury. The remaining USDC is deposited into the main vault as collateral. The user receives freshly minted pCALL tokens in return, and their minting position is recorded on-chain.

### Redeeming pCALL for Profit

When the underlying asset’s price exceeds the strike price, holders of pCALL can redeem their tokens for profit. The payout is calculated based on the price difference above the strike price, adjusted for the amount of tokens and a fixed decimal scale. A redemption fee is applied and routed to the treasury. The pCALL tokens are burned during this process, and users receive their USDC payout.

Redemption is restricted to within a fixed time window (e.g., 90 days) after the mint to prevent indefinite claims and abuse.

### Liquidation

If the vault's USDC balance becomes insufficient to cover potential redemptions due to price volatility, any user can liquidate the protocol. During liquidation, the remaining collateral is transferred to the liquidator, and the undercollateralized position is cleared.

### Administrative Controls

The protocol includes admin-only functions to:

- Update the strike price
- Pause or unpause all minting/redeeming activity

These controls allow for emergency intervention and economic rebalancing.

### Oracle Integration

The program integrates with an on-chain price oracle to fetch current asset prices. The oracle is used during redemption and liquidation to determine payouts and enforce collateral safety.

### Payout Estimation

A view-only function allows users or frontends to calculate the expected payout for a given pCALL amount based on the current oracle price.

---

## Account Model

- **Config**: Stores global protocol settings including the strike price, collateralization ratio, paused status, and authority.
- **Position**: Represents an individual user's minting state including amount of pCALL issued and timestamp.
- **Vault**: Holds USDC collateral backing all minted pCALL options.
- **Treasury Vault**: Receives protocol fees from minting and redemption.
- **pCALL Mint**: The SPL token representing perpetual call options.
- **Oracle**: A price feed account holding the latest asset price.

---

## Error Handling

The program enforces strict checks to maintain collateral safety and user fairness. Errors include:

- Attempting to mint or redeem while the protocol is paused.
- Redeeming after the option has expired.
- Redeeming when price is below strike.
- Failing collateralization ratio checks.
- Liquidation conditions not being met.

---

## Use Cases

- **Leverage**: Gain leveraged upside exposure to a target asset without the risk of liquidation.
- **Structured Products**: Integrate with other protocols or DeFi products to create covered calls or synthetic option strategies.
- **Trading**: Enable secondary markets to trade pCALL tokens as a perpetual derivative.
