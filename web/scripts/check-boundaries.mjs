import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
for (const required of ['AuthMiniProvider', 'LinkitProvider', 'LinkitAppHeaderUser', 'LinkitUserPicker', 'QueryClientProvider', 'HashRouter', '<Toaster position="top-center" closeButton />', '/api/deposits/confirm', '/api/transfers', '/api/withdrawals', '/address-book`, { method: "POST"', 'destination_address', 'Idempotency-Key']) if (!source.includes(required)) throw new Error(`Missing Midas integration: ${required}`);
if (!source.includes('linkit-react-components/styles.css')) throw new Error('LinkitUserPicker requires its published component stylesheet.');
console.log('Midas web integration check passed');
