import { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { AuthMiniProvider } from 'auth-mini-react-components';
import { LinkitProvider } from 'linkit-react-components';
import { ArrowDownToLine, ArrowUpFromLine, ArrowRightLeft, Copy, Landmark, ShieldCheck, WalletCards } from 'lucide-react';
import 'linkit-react-components/styles.css';
import './styles.css';

type AuthConfig = { auth_mini_base_url: string; audiences: string[]; linkit_base_url: string };
type Health = { ok: boolean; service: string; custody_execution: string };
const emptyConfig: AuthConfig = { auth_mini_base_url: 'https://auth.ntnl.io', audiences: ['midas.ntnl.io'], linkit_base_url: 'https://linkit.ntnl.io' };

function App() {
  const [config, setConfig] = useState<AuthConfig>(emptyConfig);
  const [health, setHealth] = useState<Health | null>(null);
  const [copied, setCopied] = useState(false);
  useEffect(() => { void fetch('/api/auth/config').then(r => r.json()).then(setConfig).catch(() => undefined); void fetch('/api/health').then(r => r.json()).then(setHealth).catch(() => undefined); }, []);
  const copyAddress = async () => { await navigator.clipboard?.writeText('Available after secure wallet provisioning'); setCopied(true); window.setTimeout(() => setCopied(false), 1500); };
  return <AuthMiniProvider authMiniBaseUrl={config.auth_mini_base_url} audiences={config.audiences} autoRedirectToLogin={false}>
    <LinkitProvider linkitBaseUrl={config.linkit_base_url}><main className="shell">
      <header className="topbar"><a className="brand" href="/"><span className="brand-mark">M</span><span>Midas</span></a><span className="environment">USD ledger · v1</span></header>
      <section className="hero"><p className="eyebrow">Stablecoin settlement infrastructure</p><h1>One USD balance. Clear payment rails.</h1><p className="lede">Midas records USD-denominated balances for EVM USDC and USDT. On-chain execution remains deliberately disabled until secure custody is configured.</p></section>
      <section className="balance" aria-label="USD balance"><div><span className="label">Available balance</span><strong>$0.00</strong><p>No posted ledger entries yet.</p></div><span className="currency">USD</span></section>
      <section className="actions" aria-label="Account actions"><Action icon={<ArrowDownToLine/>} title="Deposit" detail="Get your dedicated EVM address after secure provisioning."/><Action icon={<ArrowUpFromLine/>} title="Withdraw" detail="Disabled until an approved execution policy is configured." disabled/><Action icon={<ArrowRightLeft/>} title="Transfer" detail="Move USD ledger balance to another Midas user." disabled/></section>
      <section className="panel"><div className="panel-heading"><div><span className="eyebrow">Your deposit address</span><h2>Wallet provisioning required</h2></div><WalletCards aria-hidden="true"/></div><p>Each user will receive a unique EVM deposit address. Midas v1 does not generate, import, or expose private keys.</p><button className="address" onClick={() => void copyAddress()}><code>Not provisioned</code><Copy size={16}/><span className="sr-only">Copy address status</span></button>{copied && <p className="notice">Address status copied.</p>}</section>
      <section className="panel ledger"><div className="panel-heading"><div><span className="eyebrow">Activity</span><h2>Ledger history</h2></div><Landmark aria-hidden="true"/></div><div className="empty"><ShieldCheck size={22}/><div><strong>No transactions</strong><p>Deposits, withdrawals and transfers will appear here as immutable USD ledger entries.</p></div></div></section>
      <section className="safety"><ShieldCheck aria-hidden="true"/><p><strong>Safety boundary:</strong> private-key handling, chain listening, sweeping, withdrawals and transfers are disabled in this v1 skeleton. The API only exposes read-only balances, addresses and ledger history.</p></section>
      <footer>{health?.ok ? 'API healthy · custody execution disabled' : 'Checking API status'} · Auth Mini + Linkit integration boundary</footer>
    </main></LinkitProvider>
  </AuthMiniProvider>;
}
function Action({ icon, title, detail, disabled=false }: {icon:React.ReactNode;title:string;detail:string;disabled?:boolean}) { return <button className="action" disabled={disabled}><span>{icon}</span><strong>{title}</strong><small>{detail}</small>{disabled && <em>Not configured</em>}</button>; }
createRoot(document.getElementById('root')!).render(<App/>);
