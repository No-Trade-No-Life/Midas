import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
for (const required of ['AuthMiniProvider', 'LinkitProvider', 'LinkitAppHeaderUser', 'LinkitUserPicker', 'QueryClientProvider', 'HashRouter', '/api/deposits/confirm', '/api/transfers', '/api/withdrawals', '/api/address-book/me', 'Idempotency-Key']) if (!source.includes(required)) throw new Error(`Missing Midas integration: ${required}`);
if (!source.includes('linkit-react-components/styles.css')) throw new Error('LinkitUserPicker requires its published component stylesheet.');
console.log('Midas web integration check passed');
