import { test } from 'node:test';
import assert from 'node:assert/strict';
import { withdrawalAmountFits } from '../src/lib/withdrawal-availability.ts';
const ready = { asset_id: '1-USDC', status: 'ready', gas_sufficient: true, available_usd_nanos: '1000000001' };
test('gates exact capacity including a single nanodollar', () => {
  assert.equal(withdrawalAmountFits(ready, '1-USDC', 1000000001), true);
  assert.equal(withdrawalAmountFits(ready, '1-USDC', 1000000002), false);
  assert.equal(withdrawalAmountFits({ ...ready, available_usd_nanos: '9223372036854775807' }, '1-USDC', Number.MAX_SAFE_INTEGER), true);
  assert.equal(withdrawalAmountFits(ready, '1-USDC', Number.MAX_SAFE_INTEGER + 1), false);
});
test('unavailable, loading, zero amount, no gas, and old chain selection cannot submit', () => {
  assert.equal(withdrawalAmountFits(undefined, '1-USDC', 1), false);
  assert.equal(withdrawalAmountFits(ready, '56-USDT', 1), false);
  assert.equal(withdrawalAmountFits(ready, '1-USDC', 0), false);
  assert.equal(withdrawalAmountFits({ ...ready, gas_sufficient: false }, '1-USDC', 1), false);
  assert.equal(withdrawalAmountFits({ ...ready, status: 'unavailable' }, '1-USDC', 1), false);
});
