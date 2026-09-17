export type WithdrawalAvailability = { asset_id: string; chain_id: number; network_name: string; asset_symbol: string; available_amount: string; available_usd_nanos: string; status: "ready" | "unavailable"; gas_sufficient: boolean | null; observed_at: string | null; reason: "not_configured" | "rpc_unavailable" | "pending_gas_unknown" | "insufficient_gas" | "insufficient_liquidity" | null }

export function withdrawalAmountFits(capacity: WithdrawalAvailability | undefined, assetId: string, amountNanos: number): boolean {
  return Boolean(capacity && capacity.asset_id === assetId && capacity.status === "ready" && capacity.gas_sufficient === true && Number.isSafeInteger(amountNanos) && amountNanos > 0 && BigInt(amountNanos) <= BigInt(capacity.available_usd_nanos))
}
