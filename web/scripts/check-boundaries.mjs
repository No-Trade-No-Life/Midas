import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
for (const required of ['AuthMiniProvider', 'LinkitProvider', 'QueryClientProvider', 'HashRouter', '/api/deposits/confirm', '/api/transfers', '/api/withdrawals', 'Idempotency-Key']) if (!source.includes(required)) throw new Error(`Missing Midas integration: ${required}`);
if (source.includes('linkit-react-components/styles.css')) throw new Error('Linkit React Components must use the app shadcn/Tailwind surface, not a package stylesheet.');
console.log('Midas web integration check passed');
