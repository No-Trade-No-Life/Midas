import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../src/main.tsx', import.meta.url), 'utf8');
for (const required of ['AuthMiniProvider', 'autoRedirectToLogin', 'LinkitProvider', 'LinkitMyInfo', 'LinkitUserPicker', 'QRCodeSVG', 'qrcode.react', 'QueryClientProvider', 'HashRouter', '<Toaster position="top-center" closeButton />', '/api/admin/deposit-discovery', '/api/deposits/claim', 'depositDiscovery', '/api/transfers', '/api/withdrawals', '/api/withdrawal-targets/me', '/api/agreements/owned', '/api/agreements/bindings/me', '/api-key', '/sign_agreement', 'destination_address', 'Idempotency-Key', 'automaticChargeHelpTitle', '$MIDAS_API_KEY', 'amount_usd_micros']) if (!source.includes(required)) throw new Error(`Missing Midas integration: ${required}`);
if (!source.includes('linkit-react-components/styles.css')) throw new Error('LinkitUserPicker requires its published component stylesheet.');
if (!source.includes('autoRedirectToLogin>') || source.includes('autoRedirectToLogin={false}')) throw new Error('Midas must enable Auth Mini automatic redirect for unauthenticated visitors.');
if (source.includes('function Welcome')) throw new Error('Midas must redirect unauthenticated visitors through Auth Mini instead of rendering a local welcome page.');
if (source.includes('LinkitAppHeaderUser')) throw new Error('Midas must use the zero-prop LinkitMyInfo component.');
if (source.includes('id="transaction-hash"')) throw new Error('The customer deposit GUI must not require a transaction hash.');
console.log('Midas web integration check passed');
