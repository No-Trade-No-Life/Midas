import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { AuthMiniButton, AuthMiniProvider, useAuthMini } from "auth-mini-react-components"
import { LinkitAppHeaderUser, LinkitProvider, LinkitUserPicker } from "linkit-react-components"
import { ThemeProvider } from "next-themes"
import { HashRouter, Link, Route, Routes, useLocation } from "react-router-dom"
import { toast } from "sonner"
import {
  ArrowDownToLineIcon,
  ArrowLeftRightIcon,
  ArrowUpFromLineIcon,
  ClipboardCopyIcon,
  Globe2Icon,
  HistoryIcon,
  LandmarkIcon,
  LoaderCircleIcon,
  SettingsIcon,
  ShieldCheckIcon,
  WalletCardsIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardAction, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"
import { Drawer, DrawerContent, DrawerDescription, DrawerFooter, DrawerHeader, DrawerTitle } from "@/components/ui/drawer"
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import "linkit-react-components/styles.css"
import "./styles.css"

type Language = "en" | "zh"
type AuthConfig = { auth_mini_base_url: string; audiences: string[]; linkit_base_url: string }
type SetupStatus = { initialized: boolean; root_user_id: string | null }
type Balance = { currency: "USD"; available_usd_micros: number; available_usd: string }
type Network = { chain_id: number; name: string }
type Asset = { id: string; chain_id: number; symbol: "USDC" | "USDT"; contract_address: string; token_decimals: number; enabled: boolean; network_name?: string }
type WalletAddress = { chain_id: number; address: string; custody_status: string; created_at: string }
type LedgerEntry = { id: string; kind: string; status: string; amount_usd_micros: number; balance_delta_usd_micros: number; created_at: string; asset_symbol: string | null; external_reference: string | null; note: string | null }
type Deposit = { id: string; amount_usd: string; amount_usd_micros: number; asset_symbol: string; transaction_hash: string; sweep_status: string }
type AddressBookEntry = { id: string; chain_id: number; chain_name: string; label: string; address: string; created_at: string; updated_at: string }
type Withdrawal = { id: string; asset_symbol: string; destination_address: string; address_book_entry_id: string | null; destination_label: string | null; amount_usd: string; amount_usd_micros: number; transaction_hash: string | null; status: string }
type EvmConfig = { custody_wallet_address: string | null; custody_wallet_private_key_configured: boolean; networks: Network[]; assets: Asset[] }

const messages = {
  en: {
    appName: "Midas", tagline: "USD payment infrastructure", signIn: "Sign in", home: "Home", activity: "Activity", settings: "Settings", language: "Language", english: "English", chinese: "中文",
    welcomeTitle: "A single USD balance for EVM stablecoin payments", welcomeBody: "Sign in to receive a dedicated deposit address, move USD between Midas accounts, and view every ledger entry.",
    balance: "Available balance", usdOnly: "USD ledger", deposit: "Deposit", transfer: "Transfer", withdraw: "Withdraw", depositAddress: "Your deposit address", depositAddressHint: "Choose a network to view or create your dedicated address.", provisionAddress: "Create address", copy: "Copy", copied: "Address copied", noAddress: "Choose a configured network to create your dedicated address.",
    noActivity: "No activity yet", noActivityBody: "Confirmed deposits, transfers, and withdrawals appear here as immutable USD entries.", amount: "Amount", status: "Status", asset: "Asset", time: "Time", reference: "Reference", details: "Details",
    depositTitle: "Confirm a deposit", depositBody: "Choose a supported network and asset, send to your dedicated address, then submit the confirmed transaction hash. Midas verifies that exact receipt; it does not scan the chain.", network: "Network", transactionHash: "Transaction hash", confirmDeposit: "Confirm deposit", walletNeeded: "Create a deposit address before confirming a transaction.",
    transferTitle: "Transfer USD", transferBody: "Search the Linkit directory by username, then send USD atomically to an existing Midas account.", recipient: "Recipient", recipientHint: "Search a Linkit username. The Midas API independently verifies that the recipient has opened an account.", usdAmount: "USD amount", sendTransfer: "Send transfer", profile: "Linkit profile", unknownProfile: "No Linkit profile found; the Midas API still validates the recipient.",
    withdrawTitle: "Withdraw stablecoin", withdrawBody: "Choose a saved address on the same network as the selected asset. Midas never sends to a pasted, unsaved address.", destination: "Destination", requestWithdrawal: "Request withdrawal", finalize: "Finalize", noWithdrawals: "No withdrawals yet", awaitingSigner: "Awaiting custody signer", submitted: "Submitted", completed: "Completed", failed: "Failed", addressBook: "Withdrawal address book", addressBookBody: "Add and label approved destination addresses before requesting a withdrawal.", addressLabel: "Address label", addAddress: "Add address", addressAdded: "Address added", renameAddress: "Rename", saveAddress: "Save", removeAddress: "Remove", noAddresses: "No saved addresses on this network. Add one to continue.", savedDestination: "Saved destination",
    setupTitle: "Initialize Midas", setupBody: "The first authenticated user becomes the root operator. This action can only happen once.", initialize: "Initialize as root", initialized: "Midas is initialized", rootOnly: "Only the root operator can edit EVM custody configuration.",
    rootConfig: "Custody wallet", rootConfigBody: "Midas has built-in Ethereum, BNB Smart Chain, Base, Arbitrum, OP Mainnet, and Polygon USDC/USDT mappings and public RPCs. Enter one private key; Midas derives its address and uses it for gas, collection, and withdrawals.", custodyPrivateKey: "Custody wallet private key", custodyAddress: "Custody wallet address", saveConfiguration: "Save configuration", secretsConfigured: "Custody wallet configured", noNetworks: "No supported EVM network is available.",
    loading: "Loading account…", apiError: "We could not complete that request. Check the details and try again.", openApi: "Open API", security: "Authenticated with Auth Mini · identity surfaces from Linkit", confirm: "Confirm", cancel: "Cancel", refresh: "Refresh", wallet: "Wallet",
  },
  zh: {
    appName: "Midas", tagline: "USD 支付基础设施", signIn: "登录", home: "首页", activity: "流水", settings: "设置", language: "语言", english: "English", chinese: "中文",
    welcomeTitle: "面向 EVM 稳定币支付的统一 USD 余额", welcomeBody: "登录后可获取专属充值地址、在 Midas 账户间划转 USD，并查看全部账本流水。",
    balance: "可用余额", usdOnly: "USD 账本", deposit: "充值", transfer: "转账", withdraw: "提现", depositAddress: "你的充值地址", depositAddressHint: "选择网络以查看或创建专属地址。", provisionAddress: "创建地址", copy: "复制", copied: "地址已复制", noAddress: "选择已配置网络以创建专属充值地址。",
    noActivity: "暂无流水", noActivityBody: "确认后的充值、转账和提现会以不可变的 USD 记录显示在这里。", amount: "金额", status: "状态", asset: "资产", time: "时间", reference: "参考号", details: "详情",
    depositTitle: "确认充值", depositBody: "选择支持的网络和资产，转入专属地址后提交已确认的交易哈希。Midas 只验证该笔收据，不扫描区块链。", network: "网络", transactionHash: "交易哈希", confirmDeposit: "确认充值", walletNeeded: "确认交易前请先创建充值地址。",
    transferTitle: "转账 USD", transferBody: "按 Linkit 用户名检索收款人，再原子地转入已开通 Midas 的账户。", recipient: "收款人", recipientHint: "搜索 Linkit 用户名；Midas API 会独立校验收款人已开通账户。", usdAmount: "USD 金额", sendTransfer: "发送转账", profile: "Linkit 资料", unknownProfile: "未找到 Linkit 资料；Midas API 仍会校验收款人。",
    withdrawTitle: "提现稳定币", withdrawBody: "选择与资产相同网络的已保存地址。Midas 不会向临时粘贴、未保存的地址转账。", destination: "目标地址", requestWithdrawal: "请求提现", finalize: "最终确认", noWithdrawals: "暂无提现", awaitingSigner: "等待托管钱包签名", submitted: "已提交", completed: "已完成", failed: "失败", addressBook: "提现地址簿", addressBookBody: "先添加并标记已授权的目标地址，才能发起提现。", addressLabel: "地址标签", addAddress: "添加地址", addressAdded: "地址已添加", renameAddress: "重命名", saveAddress: "保存", removeAddress: "删除", noAddresses: "此网络暂无已保存地址。请先添加地址。", savedDestination: "已保存目标地址",
    setupTitle: "初始化 Midas", setupBody: "第一位认证用户将成为根管理员。该操作只能执行一次。", initialize: "初始化为根管理员", initialized: "Midas 已初始化", rootOnly: "只有根管理员可以修改 EVM 托管配置。",
    rootConfig: "托管钱包", rootConfigBody: "Midas 已内置 Ethereum、BNB Smart Chain、Base、Arbitrum、OP Mainnet、Polygon 的 USDC/USDT 映射与公共 RPC。只需输入一个私钥，Midas 会推导地址并用于 Gas、归集和提现。", custodyPrivateKey: "托管钱包私钥", custodyAddress: "托管钱包地址", saveConfiguration: "保存配置", secretsConfigured: "托管钱包已配置", noNetworks: "暂无支持的 EVM 网络。",
    loading: "正在加载账户…", apiError: "请求未完成。请检查填写内容后重试。", openApi: "开放 API", security: "使用 Auth Mini 认证 · 使用 Linkit 身份资料", confirm: "确认", cancel: "取消", refresh: "刷新", wallet: "钱包",
  },
} as const

const fallbackConfig: AuthConfig = { auth_mini_base_url: "https://auth.ntnl.io", audiences: ["midas.ntnl.io", "linkit.ntnl.io"], linkit_base_url: "https://linkit.ntnl.io" }

class ApiError extends Error {
  constructor(readonly status: number, message: string) { super(message) }
}

function AppRoot() {
  const [config, setConfig] = useState<AuthConfig>(fallbackConfig)
  const [client] = useState(() => new QueryClient({ defaultOptions: { queries: { retry: 1, staleTime: 10_000 } } }))

  useEffect(() => {
    void fetch("/api/auth/config").then(async (response) => response.ok ? response.json() as Promise<AuthConfig> : fallbackConfig).then(setConfig).catch(() => undefined)
  }, [])

  return <ThemeProvider attribute="class" defaultTheme="light" enableSystem={false}>
    <QueryClientProvider client={client}>
      <AuthMiniProvider authMiniBaseUrl={config.auth_mini_base_url} audiences={config.audiences} autoRedirectToLogin={false}>
        <LinkitProvider linkitBaseUrl={config.linkit_base_url}>
          <HashRouter><App /></HashRouter>
        </LinkitProvider>
      </AuthMiniProvider>
    </QueryClientProvider>
  </ThemeProvider>
}

function App() {
  const [language, setLanguage] = useState<Language>(() => window.localStorage.getItem("midas-language") === "zh" ? "zh" : "en")
  const t = (key: keyof typeof messages.en) => messages[language][key]
  const auth = useAuthMini()
  const location = useLocation()
  const setup = useQuery({ queryKey: ["setup"], queryFn: () => publicRequest<SetupStatus>("/api/setup/status") })
  const currentUserId = subjectFromToken(auth.session?.accessToken ?? undefined)
  const root = Boolean(setup.data?.initialized && setup.data.root_user_id === currentUserId)

  useEffect(() => { window.localStorage.setItem("midas-language", language) }, [language])

  return <div className="min-h-dvh bg-background">
    <header className="sticky top-0 border-b bg-background">
      <div className="mx-auto flex max-w-5xl items-center justify-between gap-3 px-4 py-3 sm:px-6">
        <Link to="/" className="flex min-w-0 items-center gap-2 font-medium"><LandmarkIcon aria-hidden="true" /> <span>{t("appName")}</span><Badge variant="secondary">USD</Badge></Link>
        <DesktopNavigation currentPath={location.pathname} t={t} />
        <div className="flex items-center gap-1">
          <LanguageMenu language={language} setLanguage={setLanguage} t={t} />
          <LinkitAppHeaderUser lang={language === "zh" ? "zh-CN" : "en"} />
        </div>
      </div>
    </header>
    <main id="main-content" className="mx-auto w-full max-w-5xl px-4 py-6 pb-24 sm:px-6 sm:py-8">
      {!auth.isAuthenticated ? <Welcome t={t} language={language} /> : <Routes>
        <Route path="/" element={<Dashboard t={t} language={language} />} />
        <Route path="/activity" element={<ActivityPage t={t} />} />
        <Route path="/settings" element={<SettingsPage t={t} root={root} setup={setup.data} currentUserId={currentUserId} />} />
        <Route path="*" element={<Dashboard t={t} language={language} />} />
      </Routes>}
    </main>
    {auth.isAuthenticated && <Navigation currentPath={location.pathname} t={t} />}
  </div>
}

function Welcome({ t, language }: { t: Translate; language: Language }) {
  return <div className="mx-auto flex max-w-xl flex-col gap-6 py-12 sm:py-20">
    <div className="flex flex-col gap-3"><Badge variant="secondary">{t("tagline")}</Badge><h1 className="text-3xl font-semibold tracking-tight text-balance sm:text-4xl">{t("welcomeTitle")}</h1><p className="max-w-prose text-muted-foreground text-pretty">{t("welcomeBody")}</p></div>
    <Card><CardHeader><CardTitle>{t("security")}</CardTitle><CardDescription>{t("usdOnly")}</CardDescription></CardHeader><CardContent><Alert><ShieldCheckIcon /><AlertTitle>{t("appName")}</AlertTitle><AlertDescription>{t("welcomeBody")}</AlertDescription></Alert></CardContent><CardFooter><AuthMiniButton lang={language} /></CardFooter></Card>
  </div>
}

function Dashboard({ t, language }: { t: Translate; language: Language }) {
  const api = useApi()
  const [drawer, setDrawer] = useState<"deposit" | "transfer" | "withdraw" | null>(null)
  const balance = useQuery({ queryKey: ["balance"], queryFn: () => api<Balance>("/api/balances/me") })
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: () => api<WalletAddress[]>("/api/wallet-addresses/me") })
  const assets = useQuery({ queryKey: ["assets"], queryFn: () => api<Asset[]>("/api/assets") })
  const ledger = useQuery({ queryKey: ["ledger"], queryFn: () => api<LedgerEntry[]>("/api/ledger/me") })
  const networks = enabledNetworks(assets.data ?? [])

  if (balance.isLoading || wallets.isLoading || assets.isLoading) return <LoadingScreen t={t} />
  if (balance.error || wallets.error || assets.error) return <RequestError t={t} />

  return <div className="flex flex-col gap-6">
    <section className="flex flex-col gap-3"><div className="flex flex-wrap items-center justify-between gap-3"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("balance")}</h1></div><Badge variant="secondary">{t("usdOnly")}</Badge></div>
      <Card><CardHeader><CardTitle>{t("balance")}</CardTitle><CardDescription>{t("usdOnly")}</CardDescription><CardAction><WalletCardsIcon aria-hidden="true" /></CardAction></CardHeader><CardContent><p className="text-3xl font-semibold tracking-tight tabular-nums">${formatUsd(balance.data?.available_usd)}</p></CardContent><CardFooter className="grid grid-cols-2 gap-2 lg:flex"><Button className="col-span-2 lg:col-auto" onClick={() => setDrawer("deposit")}><ArrowDownToLineIcon data-icon="inline-start" />{t("deposit")}</Button><Button variant="outline" onClick={() => setDrawer("transfer")}><ArrowLeftRightIcon data-icon="inline-start" />{t("transfer")}</Button><Button variant="outline" onClick={() => setDrawer("withdraw")}><ArrowUpFromLineIcon data-icon="inline-start" />{t("withdraw")}</Button></CardFooter></Card>
    </section>
    <section className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)]">
      <DepositAddressCard t={t} networks={networks} wallets={wallets.data ?? []} provision={() => undefined} />
      <ActivityPreview t={t} entries={ledger.data ?? []} />
    </section>
    <DepositDrawer open={drawer === "deposit"} setOpen={(open) => !open && setDrawer(null)} t={t} assets={assets.data ?? []} wallets={wallets.data ?? []} />
    <TransferDrawer open={drawer === "transfer"} setOpen={(open) => !open && setDrawer(null)} t={t} language={language} />
    <WithdrawalDrawer open={drawer === "withdraw"} setOpen={(open) => !open && setDrawer(null)} t={t} assets={assets.data ?? []} />
  </div>
}

function DepositAddressCard({ t, networks, wallets }: { t: Translate; networks: Network[]; wallets: WalletAddress[]; provision: () => void }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [chainId, setChainId] = useState<string>(() => networks[0] ? String(networks[0].chain_id) : "")
  const provision = useMutation({ mutationFn: () => api<WalletAddress>("/api/wallet-addresses/me", { method: "POST", body: { chain_id: Number(chainId) } }), onSuccess: () => { void queryClient.invalidateQueries({ queryKey: ["wallets"] }); toast.success(t("provisionAddress")) }, onError: () => toast.error(t("apiError")) })
  const address = wallets.find((wallet) => wallet.chain_id === Number(chainId))
  const items = networks.map((network) => ({ value: String(network.chain_id), label: network.name }))

  return <Card><CardHeader><CardTitle>{t("depositAddress")}</CardTitle><CardDescription>{t("depositAddressHint")}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">
    {items.length ? <FieldGroup><Field><FieldLabel>{t("network")}</FieldLabel><Select items={items} value={chainId || null} onValueChange={(value) => setChainId(value ?? "")}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{items.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field></FieldGroup> : <Alert><AlertTitle>{t("noNetworks")}</AlertTitle><AlertDescription>{t("rootOnly")}</AlertDescription></Alert>}
    {address ? <div className="flex items-center gap-2"><code className="min-w-0 flex-1 truncate rounded-md bg-muted px-3 py-2 text-sm">{address.address}</code><Button aria-label={t("copy")} size="icon" variant="outline" onClick={() => void navigator.clipboard.writeText(address.address).then(() => toast.success(t("copied")))}><ClipboardCopyIcon /></Button></div> : items.length ? <Button variant="outline" disabled={provision.isPending} onClick={() => provision.mutate()}>{provision.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("provisionAddress")}</Button> : null}
  </CardContent></Card>
}

function ActivityPreview({ t, entries }: { t: Translate; entries: LedgerEntry[] }) {
  return <Card><CardHeader><CardTitle>{t("activity")}</CardTitle><CardDescription>{t("details")}</CardDescription><CardAction><Button size="sm" variant="ghost" render={<Link to="/activity" />} nativeButton={false}><HistoryIcon data-icon="inline-start" />{t("activity")}</Button></CardAction></CardHeader><CardContent>{entries.length ? <LedgerTable t={t} entries={entries.slice(0, 5)} compact /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><HistoryIcon /></EmptyMedia><EmptyTitle>{t("noActivity")}</EmptyTitle><EmptyDescription>{t("noActivityBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card>
}

function ActivityPage({ t }: { t: Translate }) {
  const api = useApi()
  const ledger = useQuery({ queryKey: ["ledger"], queryFn: () => api<LedgerEntry[]>("/api/ledger/me") })
  if (ledger.isLoading) return <LoadingScreen t={t} />
  if (ledger.error) return <RequestError t={t} />
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("activity")}</h1></div><Card><CardHeader><CardTitle>{t("activity")}</CardTitle><CardDescription>{t("noActivityBody")}</CardDescription></CardHeader><CardContent>{ledger.data?.length ? <LedgerTable t={t} entries={ledger.data} /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><HistoryIcon /></EmptyMedia><EmptyTitle>{t("noActivity")}</EmptyTitle><EmptyDescription>{t("noActivityBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card></section>
}

function LedgerTable({ t, entries, compact = false }: { t: Translate; entries: LedgerEntry[]; compact?: boolean }) {
  return <><div className="flex flex-col sm:hidden">{entries.map((entry, index) => <div key={entry.id}><div className="flex items-start justify-between gap-3 py-3"><div className="min-w-0"><div className="flex items-center gap-2"><span className="font-medium capitalize">{entry.kind.replace("_", " ")}</span><Badge variant="outline">{entry.asset_symbol ?? "USD"}</Badge></div><span className="block truncate text-muted-foreground text-xs">{entry.note ?? entry.external_reference ?? entry.id}</span><span className="text-muted-foreground text-xs">{new Date(entry.created_at).toLocaleString()}</span></div><div className="flex shrink-0 flex-col items-end gap-1"><span className="font-medium tabular-nums">{entry.balance_delta_usd_micros < 0 ? "−" : "+"}${formatMicros(Math.abs(entry.balance_delta_usd_micros))}</span><StatusBadge status={entry.status} /></div></div>{index < entries.length - 1 ? <Separator /> : null}</div>)}</div><div className="hidden sm:block"><Table><TableHeader><TableRow><TableHead>{t("details")}</TableHead><TableHead>{t("asset")}</TableHead><TableHead>{t("status")}</TableHead><TableHead className="text-right">{t("amount")}</TableHead>{!compact && <TableHead>{t("time")}</TableHead>}</TableRow></TableHeader><TableBody>{entries.map((entry) => <TableRow key={entry.id}><TableCell><div className="flex flex-col gap-1"><span className="font-medium capitalize">{entry.kind.replace("_", " ")}</span><span className="max-w-40 truncate text-muted-foreground text-xs">{entry.note ?? entry.external_reference ?? entry.id}</span></div></TableCell><TableCell>{entry.asset_symbol ?? "USD"}</TableCell><TableCell><StatusBadge status={entry.status} /></TableCell><TableCell className="text-right"><span className="font-medium tabular-nums">{entry.balance_delta_usd_micros < 0 ? "−" : "+"}${formatMicros(Math.abs(entry.balance_delta_usd_micros))}</span></TableCell>{!compact && <TableCell className="whitespace-nowrap"><span className="text-muted-foreground">{new Date(entry.created_at).toLocaleString()}</span></TableCell>}</TableRow>)}</TableBody></Table></div></>
}

function DepositDrawer({ open, setOpen, t, assets, wallets }: { open: boolean; setOpen: (open: boolean) => void; t: Translate; assets: Asset[]; wallets: WalletAddress[] }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const networks = enabledNetworks(assets)
  const [chainId, setChainId] = useState("")
  const [assetId, setAssetId] = useState("")
  const [transactionHash, setTransactionHash] = useState("")
  const availableAssets = assets.filter((asset) => asset.enabled && String(asset.chain_id) === chainId)
  const submit = useMutation({ mutationFn: () => api<Deposit>("/api/deposits/confirm", { method: "POST", idempotency: crypto.randomUUID(), body: { chain_id: Number(chainId), asset_id: assetId, transaction_hash: transactionHash } }), onSuccess: (deposit) => { toast.success(`${t("confirmDeposit")}: $${deposit.amount_usd}`); void queryClient.invalidateQueries({ queryKey: ["balance"] }); void queryClient.invalidateQueries({ queryKey: ["ledger"] }); setOpen(false) }, onError: () => toast.error(t("apiError")) })
  const networkItems = networks.map((network) => ({ value: String(network.chain_id), label: network.name }))
  const assetItems = availableAssets.map((asset) => ({ value: asset.id, label: asset.symbol }))
  const addressExists = wallets.some((wallet) => wallet.chain_id === Number(chainId))

  return <Drawer open={open} onOpenChange={setOpen}><DrawerContent><DrawerHeader><DrawerTitle>{t("depositTitle")}</DrawerTitle><DrawerDescription>{t("depositBody")}</DrawerDescription></DrawerHeader><form onSubmit={(event) => { event.preventDefault(); submit.mutate() }} className="flex min-h-0 flex-1 flex-col"><div className="overflow-y-auto p-4"><FieldGroup><Field><FieldLabel>{t("network")}</FieldLabel><Select items={networkItems} value={chainId || null} onValueChange={(value) => { setChainId(value ?? ""); setAssetId("") }}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{networkItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel>{t("asset")}</FieldLabel><Select items={assetItems} value={assetId || null} onValueChange={(value) => setAssetId(value ?? "")}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{assetItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field data-invalid={transactionHash.length > 0 && !/^0x[a-fA-F0-9]{64}$/.test(transactionHash)}><FieldLabel htmlFor="transaction-hash">{t("transactionHash")}</FieldLabel><Input id="transaction-hash" value={transactionHash} onChange={(event) => setTransactionHash(event.target.value.trim())} aria-invalid={transactionHash.length > 0 && !/^0x[a-fA-F0-9]{64}$/.test(transactionHash)} placeholder="0x…" /><FieldDescription>{addressExists ? t("depositBody") : t("walletNeeded")}</FieldDescription></Field></FieldGroup></div><DrawerFooter><Button type="submit" disabled={submit.isPending || !addressExists || !assetId || !/^0x[a-fA-F0-9]{64}$/.test(transactionHash)}>{submit.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("confirmDeposit")}</Button><Button type="button" variant="outline" onClick={() => setOpen(false)}>{t("cancel")}</Button></DrawerFooter></form></DrawerContent></Drawer>
}

function TransferDrawer({ open, setOpen, t, language }: { open: boolean; setOpen: (open: boolean) => void; t: Translate; language: Language }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [recipientUserId, setRecipientUserId] = useState("")
  const [amount, setAmount] = useState("")
  const [note, setNote] = useState("")
  const submit = useMutation({ mutationFn: () => api("/api/transfers", { method: "POST", idempotency: crypto.randomUUID(), body: { recipient_user_id: recipientUserId, amount_usd_micros: usdToMicros(amount), note: note || undefined } }), onSuccess: () => { toast.success(t("sendTransfer")); void queryClient.invalidateQueries({ queryKey: ["balance"] }); void queryClient.invalidateQueries({ queryKey: ["ledger"] }); setOpen(false) }, onError: () => toast.error(t("apiError")) })
  const validAmount = usdToMicros(amount) > 0

  return <Drawer open={open} onOpenChange={setOpen}><DrawerContent><DrawerHeader><DrawerTitle>{t("transferTitle")}</DrawerTitle><DrawerDescription>{t("transferBody")}</DrawerDescription></DrawerHeader><form onSubmit={(event) => { event.preventDefault(); submit.mutate() }} className="flex min-h-0 flex-1 flex-col"><div className="overflow-y-auto p-4"><FieldGroup><Field><LinkitUserPicker name="recipient_user_id" value={recipientUserId} onValueChange={(userId) => setRecipientUserId(userId)} label={t("recipient")} lang={language === "zh" ? "zh-CN" : "en"} required /><FieldDescription>{t("recipientHint")}</FieldDescription></Field><UsdField t={t} amount={amount} setAmount={setAmount} /><Field><FieldLabel htmlFor="transfer-note">{t("details")}</FieldLabel><Input id="transfer-note" value={note} onChange={(event) => setNote(event.target.value)} maxLength={280} /></Field></FieldGroup></div><DrawerFooter><Button type="submit" disabled={submit.isPending || !recipientUserId || !validAmount}>{submit.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("sendTransfer")}</Button><Button type="button" variant="outline" onClick={() => setOpen(false)}>{t("cancel")}</Button></DrawerFooter></form></DrawerContent></Drawer>
}

function WithdrawalDrawer({ open, setOpen, t, assets }: { open: boolean; setOpen: (open: boolean) => void; t: Translate; assets: Asset[] }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [assetId, setAssetId] = useState("")
  const [addressBookEntryId, setAddressBookEntryId] = useState("")
  const [amount, setAmount] = useState("")
  const addressBook = useQuery({ queryKey: ["address-book"], queryFn: () => api<AddressBookEntry[]>("/api/address-book/me") })
  const assetItems = assets.filter((asset) => asset.enabled).map((asset) => ({ value: asset.id, label: asset.symbol }))
  const selectedAsset = assets.find((asset) => asset.id === assetId)
  const destinationItems = (addressBook.data ?? []).filter((entry) => entry.chain_id === selectedAsset?.chain_id).map((entry) => ({ value: entry.id, label: `${entry.label} · ${shortAddress(entry.address)}` }))
  const submit = useMutation({ mutationFn: () => api<Withdrawal>("/api/withdrawals", { method: "POST", idempotency: crypto.randomUUID(), body: { asset_id: assetId, address_book_entry_id: addressBookEntryId, amount_usd_micros: usdToMicros(amount) } }), onSuccess: () => { toast.success(t("requestWithdrawal")); void queryClient.invalidateQueries({ queryKey: ["balance"] }); void queryClient.invalidateQueries({ queryKey: ["withdrawals"] }); void queryClient.invalidateQueries({ queryKey: ["ledger"] }); setOpen(false) }, onError: () => toast.error(t("apiError")) })
  return <Drawer open={open} onOpenChange={setOpen}><DrawerContent><DrawerHeader><DrawerTitle>{t("withdrawTitle")}</DrawerTitle><DrawerDescription>{t("withdrawBody")}</DrawerDescription></DrawerHeader><form onSubmit={(event) => { event.preventDefault(); submit.mutate() }} className="flex min-h-0 flex-1 flex-col"><div className="overflow-y-auto p-4"><FieldGroup><Field><FieldLabel>{t("asset")}</FieldLabel><Select items={assetItems} value={assetId || null} onValueChange={(value) => { setAssetId(value ?? ""); setAddressBookEntryId("") }}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{assetItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field data-invalid={Boolean(assetId) && !addressBookEntryId}><FieldLabel>{t("savedDestination")}</FieldLabel><Select items={destinationItems} value={addressBookEntryId || null} onValueChange={(value) => setAddressBookEntryId(value ?? "")} disabled={!selectedAsset || addressBook.isLoading}><SelectTrigger className="w-full" aria-invalid={Boolean(assetId) && !addressBookEntryId}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{destinationItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select>{selectedAsset && !addressBook.isLoading && !destinationItems.length ? <FieldDescription>{t("noAddresses")}</FieldDescription> : null}</Field><UsdField t={t} amount={amount} setAmount={setAmount} /></FieldGroup></div><DrawerFooter><Button type="submit" disabled={submit.isPending || !assetId || !addressBookEntryId || usdToMicros(amount) <= 0}>{submit.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("requestWithdrawal")}</Button><Button type="button" variant="outline" onClick={() => setOpen(false)}>{t("cancel")}</Button></DrawerFooter></form></DrawerContent></Drawer>
}

function UsdField({ t, amount, setAmount }: { t: Translate; amount: string; setAmount: (value: string) => void }) {
  const invalid = amount.length > 0 && usdToMicros(amount) <= 0
  return <Field data-invalid={invalid}><FieldLabel htmlFor="usd-amount">{t("usdAmount")}</FieldLabel><Input id="usd-amount" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} aria-invalid={invalid} placeholder="0.000000" /><FieldDescription>USD · 6 decimals</FieldDescription></Field>
}

function SettingsPage({ t, root, setup, currentUserId }: { t: Translate; root: boolean; setup?: SetupStatus; currentUserId: string | null }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const initialize = useMutation({ mutationFn: () => api<SetupStatus>("/api/setup/initialize", { method: "POST", body: { root_user_id: currentUserId } }), onSuccess: () => { toast.success(t("initialized")); void queryClient.invalidateQueries({ queryKey: ["setup"] }) }, onError: () => toast.error(t("apiError")) })
  const withdrawals = useQuery({ queryKey: ["withdrawals"], queryFn: () => api<Withdrawal[]>("/api/withdrawals/me"), enabled: Boolean(setup?.initialized) })
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("settings")}</h1></div>
    {!setup?.initialized ? <Card><CardHeader><CardTitle>{t("setupTitle")}</CardTitle><CardDescription>{t("setupBody")}</CardDescription></CardHeader><CardFooter><Button disabled={!currentUserId || initialize.isPending} onClick={() => initialize.mutate()}>{initialize.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("initialize")}</Button></CardFooter></Card> : null}
    {root ? <RootConfig t={t} /> : setup?.initialized ? <Alert><ShieldCheckIcon /><AlertTitle>{t("rootOnly")}</AlertTitle><AlertDescription>{t("security")}</AlertDescription></Alert> : null}
    {setup?.initialized ? <AddressBookCard t={t} /> : null}
    <Card><CardHeader><CardTitle>{t("withdraw")}</CardTitle><CardDescription>{t("withdrawBody")}</CardDescription></CardHeader><CardContent>{withdrawals.isLoading ? <Skeleton className="h-20 w-full" /> : withdrawals.data?.length ? <div className="flex flex-col gap-3">{withdrawals.data.map((withdrawal) => <WithdrawalRow key={withdrawal.id} withdrawal={withdrawal} t={t} />)}</div> : <Empty><EmptyHeader><EmptyMedia variant="icon"><ArrowUpFromLineIcon /></EmptyMedia><EmptyTitle>{t("noWithdrawals")}</EmptyTitle><EmptyDescription>{t("withdrawBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card>
  </section>
}

function WithdrawalRow({ withdrawal, t }: { withdrawal: Withdrawal; t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const finalize = useMutation({ mutationFn: () => api<Withdrawal>(`/api/withdrawals/${withdrawal.id}/finalize`, { method: "POST" }), onSuccess: () => { toast.success(t("finalize")); void queryClient.invalidateQueries({ queryKey: ["withdrawals"] }); void queryClient.invalidateQueries({ queryKey: ["balance"] }); void queryClient.invalidateQueries({ queryKey: ["ledger"] }) }, onError: () => toast.error(t("apiError")) })
  return <div className="flex flex-wrap items-center justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2"><span className="font-medium">${withdrawal.amount_usd}</span><Badge variant="outline">{withdrawal.asset_symbol}</Badge><StatusBadge status={withdrawal.status} /></div><code className="block max-w-72 truncate text-muted-foreground text-xs">{withdrawal.destination_address}</code></div>{withdrawal.status === "submitted" ? <Button size="sm" variant="outline" disabled={finalize.isPending} onClick={() => finalize.mutate()}>{finalize.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("finalize")}</Button> : null}</div>
}

function RootConfig({ t }: { t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const config = useQuery({ queryKey: ["evm-config"], queryFn: () => api<EvmConfig>("/api/admin/evm-config") })
  const [privateKey, setPrivateKey] = useState("")
  const save = useMutation({ mutationFn: () => api<EvmConfig>("/api/admin/evm-config", { method: "PUT", body: { custody_wallet_private_key: privateKey || undefined } }), onSuccess: () => { toast.success(t("saveConfiguration")); setPrivateKey(""); void queryClient.invalidateQueries({ queryKey: ["evm-config"] }) }, onError: () => toast.error(t("apiError")) })
  if (config.isLoading) return <Skeleton className="h-60 w-full" />
  if (config.error) return <RequestError t={t} />
  return <Card><CardHeader><CardTitle>{t("rootConfig")}</CardTitle><CardDescription>{t("rootConfigBody")}</CardDescription><CardAction>{config.data?.custody_wallet_private_key_configured ? <Badge variant="secondary">{t("secretsConfigured")}</Badge> : null}</CardAction></CardHeader><CardContent><form onSubmit={(event) => { event.preventDefault(); save.mutate() }}><FieldGroup>{config.data?.custody_wallet_address ? <Field><FieldLabel>{t("custodyAddress")}</FieldLabel><code className="truncate rounded-md bg-muted px-3 py-2 text-sm">{config.data.custody_wallet_address}</code></Field> : null}<Field><FieldLabel htmlFor="custody-private-key">{t("custodyPrivateKey")}</FieldLabel><Input id="custody-private-key" type="password" autoComplete="off" value={privateKey} onChange={(event) => setPrivateKey(event.target.value)} /><FieldDescription>{t("rootConfigBody")}</FieldDescription></Field><Button type="submit" disabled={save.isPending || !privateKey}>{save.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("saveConfiguration")}</Button></FieldGroup></form></CardContent></Card>
}

function AddressBookCard({ t }: { t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const assets = useQuery({ queryKey: ["assets"], queryFn: () => api<Asset[]>("/api/assets") })
  const addressBook = useQuery({ queryKey: ["address-book"], queryFn: () => api<AddressBookEntry[]>("/api/address-book/me") })
  const [form, setForm] = useState({ chainId: "", label: "", address: "" })
  const networks = enabledNetworks(assets.data ?? [])
  useEffect(() => { if (!form.chainId && networks[0]) setForm((current) => ({ ...current, chainId: String(networks[0].chain_id) })) }, [form.chainId, networks])
  const networkItems = networks.map((network) => ({ value: String(network.chain_id), label: network.name }))
  const validAddress = /^0x[a-fA-F0-9]{40}$/.test(form.address)
  const create = useMutation({ mutationFn: () => api<AddressBookEntry>("/api/address-book/me", { method: "POST", body: { chain_id: Number(form.chainId), label: form.label, address: form.address } }), onSuccess: () => { toast.success(t("addressAdded")); setForm((current) => ({ ...current, label: "", address: "" })); void queryClient.invalidateQueries({ queryKey: ["address-book"] }) }, onError: () => toast.error(t("apiError")) })
  return <Card><CardHeader><CardTitle>{t("addressBook")}</CardTitle><CardDescription>{t("addressBookBody")}</CardDescription></CardHeader><CardContent className="flex flex-col gap-5"><form onSubmit={(event) => { event.preventDefault(); create.mutate() }}><FieldGroup><Field><FieldLabel>{t("network")}</FieldLabel><Select items={networkItems} value={form.chainId || null} onValueChange={(value) => setForm({ ...form, chainId: value ?? "" })}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{networkItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="address-label">{t("addressLabel")}</FieldLabel><Input id="address-label" value={form.label} onChange={(event) => setForm({ ...form, label: event.target.value })} maxLength={80} /></Field><Field data-invalid={form.address.length > 0 && !validAddress}><FieldLabel htmlFor="address-book-address">{t("destination")}</FieldLabel><Input id="address-book-address" value={form.address} onChange={(event) => setForm({ ...form, address: event.target.value.trim() })} aria-invalid={form.address.length > 0 && !validAddress} placeholder="0x…" /></Field><Button type="submit" disabled={create.isPending || !form.chainId || !form.label.trim() || !validAddress}>{create.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("addAddress")}</Button></FieldGroup></form>{addressBook.isLoading ? <Skeleton className="h-20 w-full" /> : addressBook.data?.length ? <div className="flex flex-col gap-3">{addressBook.data.map((entry) => <AddressBookRow key={entry.id} entry={entry} t={t} />)}</div> : <Empty><EmptyHeader><EmptyMedia variant="icon"><LandmarkIcon /></EmptyMedia><EmptyTitle>{t("addressBook")}</EmptyTitle><EmptyDescription>{t("addressBookBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card>
}

function AddressBookRow({ entry, t }: { entry: AddressBookEntry; t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState(false)
  const [label, setLabel] = useState(entry.label)
  const rename = useMutation({ mutationFn: () => api<AddressBookEntry>(`/api/address-book/me/${entry.id}`, { method: "PUT", body: { label } }), onSuccess: () => { setEditing(false); void queryClient.invalidateQueries({ queryKey: ["address-book"] }) }, onError: () => toast.error(t("apiError")) })
  const remove = useMutation({ mutationFn: () => api<void>(`/api/address-book/me/${entry.id}`, { method: "DELETE" }), onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["address-book"] }), onError: () => toast.error(t("apiError")) })
  return <div className="flex flex-wrap items-center justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2">{editing ? <form onSubmit={(event) => { event.preventDefault(); rename.mutate() }}><FieldGroup className="flex-row items-center gap-2"><Field><FieldLabel className="sr-only" htmlFor={`address-book-label-${entry.id}`}>{t("addressLabel")}</FieldLabel><Input id={`address-book-label-${entry.id}`} value={label} onChange={(event) => setLabel(event.target.value)} maxLength={80} /></Field><Button size="sm" type="submit" disabled={rename.isPending || !label.trim()}>{t("saveAddress")}</Button><Button size="sm" type="button" variant="ghost" onClick={() => { setLabel(entry.label); setEditing(false) }}>{t("cancel")}</Button></FieldGroup></form> : <><span className="font-medium">{entry.label}</span><Badge variant="outline">{entry.chain_name}</Badge></>}</div><code className="block max-w-72 truncate text-xs text-muted-foreground">{entry.address}</code></div>{!editing ? <div className="flex items-center gap-1"><Button size="sm" variant="ghost" onClick={() => setEditing(true)}>{t("renameAddress")}</Button><Button size="sm" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate()}>{t("removeAddress")}</Button></div> : null}</div>
}

function DesktopNavigation({ currentPath, t }: { currentPath: string; t: Translate }) {
  const item = (path: string, label: string) => <Button key={path} size="sm" variant={currentPath === path ? "secondary" : "ghost"} render={<Link to={path} />} nativeButton={false}>{label}</Button>
  return <nav aria-label="Primary" className="hidden items-center gap-1 lg:flex">{item("/", t("home"))}{item("/activity", t("activity"))}{item("/settings", t("settings"))}</nav>
}

function Navigation({ currentPath, t }: { currentPath: string; t: Translate }) {
  const item = (path: string, label: string, icon: React.ReactNode) => <Button key={path} size="sm" variant={currentPath === path ? "secondary" : "ghost"} render={<Link to={path} />} nativeButton={false}>{icon}{label}</Button>
  return <nav aria-label="Primary" className="fixed inset-x-0 bottom-0 bg-background lg:hidden"><Separator /><div className="mx-auto flex max-w-5xl items-center justify-around gap-1 px-3 py-2">{item("/", t("home"), <WalletCardsIcon data-icon="inline-start" />)}{item("/activity", t("activity"), <HistoryIcon data-icon="inline-start" />)}{item("/settings", t("settings"), <SettingsIcon data-icon="inline-start" />)}</div></nav>
}

function LanguageMenu({ language, setLanguage, t }: { language: Language; setLanguage: (language: Language) => void; t: Translate }) {
  return <DropdownMenu><DropdownMenuTrigger render={<Button aria-label={t("language")} size="icon" variant="ghost" />}><Globe2Icon /></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuGroup><DropdownMenuItem onClick={() => setLanguage("en")}><Globe2Icon />{t("english")}{language === "en" ? <Badge variant="secondary">EN</Badge> : null}</DropdownMenuItem><DropdownMenuItem onClick={() => setLanguage("zh")}><Globe2Icon />{t("chinese")}{language === "zh" ? <Badge variant="secondary">中文</Badge> : null}</DropdownMenuItem></DropdownMenuGroup></DropdownMenuContent></DropdownMenu>
}

function StatusBadge({ status }: { status: string }) { return <Badge variant={status === "failed" || status === "rejected" ? "destructive" : status === "completed" || status === "posted" ? "secondary" : "outline"}>{status.replace("_", " ")}</Badge> }
function LoadingScreen({ t }: { t: Translate }) { return <div className="flex flex-col gap-4"><Skeleton className="h-8 w-40" /><Skeleton className="h-36 w-full" /><p className="text-muted-foreground">{t("loading")}</p></div> }
function RequestError({ t }: { t: Translate }) { return <Alert variant="destructive"><AlertTitle>{t("apiError")}</AlertTitle><AlertDescription>{t("refresh")}</AlertDescription></Alert> }

type Translate = (key: keyof typeof messages.en) => string
function useApi() { const auth = useAuthMini(); return useCallback(async <T,>(path: string, init: { method?: string; body?: unknown; idempotency?: string } = {}) => { const send = async (refresh: boolean) => { const snapshot = refresh ? await auth.sdk?.session.refresh() : auth.sdk?.session.getState(); const token = snapshot?.accessToken; if (!token) throw new ApiError(401, "Authentication is required"); const headers = new Headers({ Authorization: `Bearer ${token}` }); if (init.body !== undefined) headers.set("Content-Type", "application/json"); if (init.idempotency) headers.set("Idempotency-Key", init.idempotency); return fetch(path, { method: init.method ?? "GET", headers, body: init.body === undefined ? undefined : JSON.stringify(init.body) }) }; let response = await send(false); if (response.status === 401) response = await send(true); if (!response.ok) { const body = await response.json().catch(() => ({ error: response.statusText })) as { error?: string }; throw new ApiError(response.status, body.error ?? response.statusText) } return response.status === 204 ? undefined as T : response.json() as Promise<T> }, [auth.sdk]) }
async function publicRequest<T>(path: string) { const response = await fetch(path); if (!response.ok) throw new ApiError(response.status, response.statusText); return response.json() as Promise<T> }
function enabledNetworks(assets: Asset[]): Network[] { const networks = new Map<number, string>(); for (const asset of assets.filter((asset) => asset.enabled)) networks.set(asset.chain_id, asset.network_name ?? `Chain ${asset.chain_id}`); return [...networks].map(([chain_id, name]) => ({ chain_id, name })) }
function formatUsd(value?: string) { return value ? Number(value).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 }) : "0.00" }
function formatMicros(value: number) { return (value / 1_000_000).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 }) }
function usdToMicros(value: string) { if (!/^\d+(?:\.\d{1,6})?$/.test(value)) return 0; const [whole, fraction = ""] = value.split("."); const micros = BigInt(whole) * 1_000_000n + BigInt((fraction + "000000").slice(0, 6)); return micros <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(micros) : 0 }
function shortAddress(value: string) { return `${value.slice(0, 6)}…${value.slice(-4)}` }
function subjectFromToken(token?: string) { try { const value = token?.split(".")[1]; return value ? JSON.parse(atob(value.replace(/-/g, "+").replace(/_/g, "/"))).sub as string : null } catch { return null } }

createRoot(document.getElementById("root")!).render(<AppRoot />)
