import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react"
import { createPortal } from "react-dom"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider, type UseQueryResult, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { AuthMiniProvider, useAuthMini } from "auth-mini-react-components"
import { LinkitAppHeaderUser, LinkitProvider, LinkitUserInfo, LinkitUserPicker } from "linkit-react-components"
import { ThemeProvider } from "next-themes"
import { QRCodeSVG } from "qrcode.react"
import { HashRouter, Link, Route, Routes, useLocation } from "react-router-dom"
import { toast } from "sonner"
import {
  ArrowDownToLineIcon,
  ArrowLeftRightIcon,
  ArrowUpFromLineIcon,
  ChevronLeftIcon,
  ChevronRightIcon,
  ClipboardCopyIcon,
  ExternalLinkIcon,
  Globe2Icon,
  HistoryIcon,
  LandmarkIcon,
  LoaderCircleIcon,
  MenuIcon,
  RefreshCwIcon,
  RotateCcwIcon,
  ScrollTextIcon,
  SearchIcon,
  SettingsIcon,
  ShieldCheckIcon,
  UsersRoundIcon,
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
import { Toaster } from "@/components/ui/sonner"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import "linkit-react-components/styles.css"
import "./styles.css"

type Language = "en" | "zh"
type AuthConfig = { auth_mini_base_url: string; audiences: string[]; linkit_base_url: string }
type SetupStatus = { initialized: boolean; root_user_id: string | null }
type Balance = { currency: "USD"; available_usd_micros: number; available_usd: string }
type Network = { chain_id: number; name: string }
type Asset = { id: string; chain_id: number; symbol: "USDC" | "USDT"; contract_address: string; token_decimals: number; enabled: boolean; network_name?: string }
type WalletAddress = { address: string; created_at: string }
type LedgerEntry = { id: string; kind: string; status: string; amount_usd_micros: number; balance_delta_usd_micros: number; created_at: string; asset_symbol: string | null; chain_id: number | null; network_name: string | null; transaction_hash: string | null; external_reference: string | null; note: string | null }
type Deposit = { id: string; amount_usd: string; amount_usd_micros: number; asset_symbol: string; transaction_hash: string; sweep_status: string }
type AdminDeposit = { id: string; user_id: string; deposit_address: string; asset_symbol: string; chain_id: number; network_name: string; amount_usd_micros: number; amount_usd: string; transaction_hash: string; sweep_status: string; created_at: string; sweep_operation_status: string; gas_transaction_hash: string | null; token_transaction_hash: string | null; sweep_error_message: string | null; sweep_updated_at: string }
type AdminWithdrawal = { id: string; user_id: string; destination_address: string; asset_symbol: string; chain_id: number; network_name: string; amount_usd_micros: number; amount_usd: string; transaction_hash: string | null; status: string; last_error: string | null; created_at: string; retryable: boolean }
type AdminBalanceSummary = { user_count: number; funded_user_count: number; total_available_usd_micros: number; total_available_usd: string }
type AdminUserBalance = { user_id: string; created_at: string; available_usd_micros: number; available_usd: string }
type AdminBalances = { summary: AdminBalanceSummary; users: AdminUserBalance[] }
type AdminLedgerEntry = LedgerEntry & { user_id: string; counterparty_user_id: string | null }
type AdminLedgerPage = { entries: AdminLedgerEntry[]; total: number; limit: number; offset: number }
type AddressBookEntry = { id: string; chain_id: number; chain_name: string; label: string; address: string; created_at: string; updated_at: string }
type Withdrawal = { id: string; asset_symbol: string; destination_address: string; address_book_entry_id: string | null; destination_label: string | null; amount_usd: string; amount_usd_micros: number; transaction_hash: string | null; status: string }
type WithdrawalAddressTarget = { note_id: string | null; asset_id: string; asset_symbol: string; chain_id: number; network_name: string; address: string; label: string | null; last_withdrawn_at: string }
type PaymentAgreement = { id: string; owner_user_id: string; name: string; api_key_prefix: string; created_at: string; updated_at: string }
type PaymentAgreementCreated = { agreement: PaymentAgreement; api_key: string }
type PaymentAgreementDetail = { agreement: PaymentAgreement; bound: boolean }
type PaymentAgreementBinding = { agreement: PaymentAgreement; created_at: string }
type EvmConfig = { custody_wallet_address: string | null; custody_wallet_private_key_configured: boolean; networks: Network[]; assets: Asset[] }
type DepositDiscoveryStatus = { polling_interval_seconds: number; last_attempt_at: string | null; last_success_at: string | null; last_error: string | null }
type CustodyAssetBalance = { symbol: string; amount: string | null; error: string | null }
type CustodyNetworkBalances = { chain_id: number; network_name: string; native: CustodyAssetBalance; usdc: CustodyAssetBalance; usdt: CustodyAssetBalance }
type CustodyBalances = { custody_wallet_address: string | null; networks: CustodyNetworkBalances[] }

const messages = {
  en: {
    appName: "Midas", menu: "Menu", home: "Home", activity: "Activity", settings: "Settings", account: "Account", administration: "Administration", custody: "Custody wallet", depositDiscovery: "Deposit discovery", userBalances: "User balances", globalLedger: "Global ledger", language: "Language", english: "English", chinese: "中文",
    balance: "Available balance", usdOnly: "USD ledger", deposit: "Deposit", transfer: "Transfer", withdraw: "Withdraw", depositAddress: "Your deposit address", depositAddressHint: "This single EVM-compatible address works on every supported network.", depositQrHint: "Scan to copy this deposit address", copy: "Copy", copied: "Address copied", noAddress: "Your dedicated EVM address is being prepared. Refresh shortly.",
    noActivity: "No activity yet", noActivityBody: "Confirmed deposits, transfers, and withdrawals appear here as immutable USD entries.", amount: "Amount", status: "Status", asset: "Asset", blockchain: "Blockchain", transaction: "Transaction", action: "Action", user: "User", time: "Time", reference: "Reference", details: "Details", viewOnExplorer: "View on explorer",
    depositTitle: "Deposit USDC or USDT", depositBody: "Send any supported USDC or USDT to your dedicated address. Midas discovers the transaction automatically, then verifies its final on-chain receipt before crediting your USD balance.", depositDiscoveryDelay: "Discovery runs in the background. A deposit normally appears within about 30 seconds at the current account size.", supportedAssets: "Supported assets", network: "Network", transactionHash: "Transaction hash", confirmDeposit: "Confirm deposit", depositNotReceived: "Deposit not received?", claimDepositTitle: "Find a deposit", claimDepositBody: "Choose the network and paste the completed transaction ID. Midas derives the token and amount from the final on-chain receipt before it credits your USD balance.", claimDepositHint: "Only the network and transaction ID are needed. A transaction can be credited only once.", claimDeposit: "Verify and credit", depositClaimed: "Deposit credited",
    transferTitle: "Transfer USD", transferBody: "Search the Linkit directory by username, then send USD atomically. Midas automatically creates the recipient's account and wallet if needed.", recipient: "Recipient", recipientHint: "Search a Linkit username. The selected recipient receives a Midas account and dedicated wallet automatically.", usdAmount: "USD amount", sendTransfer: "Send transfer", profile: "Linkit profile", unknownProfile: "No Linkit profile found; choose a recipient from the Linkit directory.",
    withdrawTitle: "Withdraw stablecoin", withdrawBody: "Choose a network and token, then enter the EVM address that should receive the withdrawal.", destination: "Destination", requestWithdrawal: "Request withdrawal", finalize: "Finalize", noWithdrawals: "No withdrawals yet", awaitingSigner: "Awaiting custody signer", submitted: "Submitted", completed: "Completed", failed: "Failed", addressBook: "Saved withdrawal destinations", addressBookBody: "Save and label destinations from withdrawal history after you use them.", addressLabel: "Address label", addressAdded: "Destination saved", renameAddress: "Rename", saveAddress: "Save", saveDestination: "Save destination", removeAddress: "Remove", savedDestination: "Saved destination", withdrawalAddressBook: "Withdrawal address book", withdrawalAddressBookBody: "Destinations appear after Midas has broadcast a withdrawal. Save a note for each network and stablecoin combination.", noWithdrawalTargets: "No withdrawal destinations yet", noWithdrawalTargetsBody: "Broadcast one withdrawal first; its network, token, and destination can then be saved here.", withdrawalTargetNote: "Note", saveNote: "Save note", editNote: "Edit note", removeNote: "Remove note", withdrawalTargetPicker: "Saved destination", selectWithdrawalTarget: "Choose a past withdrawal", withdrawalTargetHint: "Selecting a destination fills its network, token, and address.",
    automaticReceipts: "Automatic receipts", automaticReceiptsBody: "Create a payment channel for an external app. Its API key can charge only users who explicitly authorize this channel.", newPaymentChannel: "New payment channel", channelName: "Channel name", createChannel: "Create channel", noPaymentChannels: "No payment channels yet", apiKey: "API key", apiKeyCreated: "Copy this API key now", apiKeyCreatedBody: "For security, Midas will not show this key again. External charges require this key and an Idempotency-Key header.", copyApiKey: "Copy API key", channelKeyPrefix: "Key prefix", authorizationLink: "Authorization link", automaticPayments: "Automatic payments", automaticPaymentsBody: "These channels can charge your available USD balance through their API key. You can revoke authorization at any time.", noAutomaticPayments: "No automatic payments", noAutomaticPaymentsBody: "When you authorize a payment channel, it will appear here.", unbind: "Revoke authorization", boundOn: "Authorized", signAgreement: "Authorize automatic payment", signAgreementBody: "Confirm that this channel may charge your available Midas USD balance with its API key.", authorizeAutomaticCharge: "Authorize channel", agreementAuthorized: "Automatic payment authorized", agreementAlreadyAuthorized: "This payment channel is already authorized.", invalidAgreementLink: "This authorization link is invalid or the payment channel no longer exists.", channelOwner: "Channel owner",
    setupTitle: "Initialize Midas", setupBody: "The first authenticated user becomes the root operator. This action can only happen once.", initialize: "Initialize as root", initialized: "Midas is initialized", rootOnly: "Only the root operator can edit EVM custody configuration.",
    rootConfig: "Custody wallet", rootConfigBody: "Midas has built-in Ethereum, BNB Smart Chain, Base, Arbitrum, OP Mainnet, and Polygon USDC/USDT mappings with verified RPC endpoints. Enter one private key; Midas derives its address and uses it for gas, collection, and withdrawals.", custodyPrivateKey: "Custody wallet private key", custodyAddress: "Custody wallet address", saveConfiguration: "Save configuration", secretsConfigured: "Custody wallet configured", noNetworks: "No supported EVM network is available.", custodyBalances: "Custody balances", custodyBalancesBody: "Live native-token, USDC, and USDT balances for the configured custody wallet on every supported EVM network.", noCustodyBalances: "Configure the custody wallet first", noCustodyBalancesBody: "Midas can read balances after it derives and stores the custody wallet address.", nativeToken: "Native token", unavailable: "Unavailable",
    rpcDiscovery: "RPC deposit discovery", rpcDiscoveryBody: "Midas polls one deposit address on one network per second through the configured chain RPCs. Each candidate Transfer is verified again from its final transaction receipt before crediting; no Etherscan API key is required.", discoveryPolling: "Polling interval", discoveryLastAttempt: "Last attempt", discoveryLastSuccess: "Last successful poll", discoveryLastError: "Last discovery error", discoveryNotStarted: "Discovery has not polled an address yet.", seconds: "seconds",
    collectionOperations: "Collection operations", collectionOperationsBody: "All credited deposits and their gas-funding and token-collection state. Only records that were not submitted can be retried.", noCollectionOperations: "No deposit collections yet", noCollectionOperationsBody: "Credited deposits will appear here with their collection status.", collectionUser: "User ID", collectionGasTransaction: "Gas transaction", collectionTokenTransaction: "Collection transaction", collectionError: "Last error", retryCollection: "Retry collection", retryCollectionTitle: "Retry this collection?", retryCollectionBody: "Midas will fund the deposit address with gas and submit its token collection transaction. Only retry after reviewing the current status and transaction references.", retryCollectionNotice: "This sends on-chain transactions and cannot be undone.", collectionRetryStarted: "Collection retry submitted", collectionQueued: "Queued", collectionAwaitingConfiguration: "Awaiting configuration", collectionSubmitted: "Submitted", collectionFailed: "Failed", collectionSwept: "Swept",
    withdrawalOperations: "Withdrawal operations", withdrawalOperationsBody: "Every withdrawal request with its chain state. A retry re-broadcasts the exact saved transaction when a TxID exists; it signs a new transaction only when no TxID exists.", noWithdrawalOperations: "No withdrawals yet", noWithdrawalOperationsBody: "Withdrawal requests appear here as soon as a customer submits them.", withdrawalDestination: "Destination", withdrawalError: "Last error", retryWithdrawal: "Retry withdrawal", retryWithdrawalTitle: "Retry this withdrawal?", retryWithdrawalBody: "Midas will either re-broadcast the exact saved transaction or, only when no TxID exists, create one new transaction.", retryWithdrawalNotice: "A withdrawal with a TxID can never be re-signed from this action.", withdrawalRetryStarted: "Withdrawal retry queued",
    userBalancesTitle: "User balances", userBalancesBody: "All Midas accounts and their currently available USD balance.", totalHeld: "Total balance held", fundedAccounts: "Funded accounts", allAccounts: "All accounts", noUsers: "No Midas accounts yet", globalLedgerTitle: "Global ledger", globalLedgerBody: "All immutable Midas ledger entries, including on-chain context when a deposit or withdrawal has a transaction.", allKinds: "All actions", allStatuses: "All statuses", transferIn: "Transfer in", transferOut: "Transfer out", adjustment: "Adjustment", pending: "Pending", posted: "Posted", rejected: "Rejected", disabled: "Disabled", filter: "Apply filters", userIdFilter: "User ID", userIdFilterHint: "Exact Auth Mini UUID", totalEntries: "Total entries", previous: "Previous", next: "Next", page: "Page", noLedgerEntries: "No ledger entries match these filters.",
    loading: "Loading account…", requestFailed: "Request failed", unknownRequestError: "Midas could not complete the request. Please try again.", evmRpcUnavailable: "Midas could not read this transaction from the blockchain. Your balance was not changed; please try again shortly.", transactionNotConfirmed: "This transaction is not confirmed on-chain yet. Please try again shortly.", transactionReverted: "This transaction reverted on-chain and cannot be credited.", depositTransferMissing: "No transfer of the selected asset to this deposit address was found. Check the network, asset, and transaction hash.", depositAlreadyCredited: "This on-chain transfer has already been credited.", openApi: "Open API", security: "Authenticated with Auth Mini · identity surfaces from Linkit", confirm: "Confirm", cancel: "Cancel", refresh: "Refresh", wallet: "Wallet",
  },
  zh: {
    appName: "Midas", menu: "菜单", home: "首页", activity: "流水", settings: "设置", account: "账户", administration: "后台管理", custody: "托管钱包", depositDiscovery: "充值发现", userBalances: "用户余额", globalLedger: "全局流水", language: "语言", english: "English", chinese: "中文",
    balance: "可用余额", usdOnly: "USD 账本", deposit: "充值", transfer: "转账", withdraw: "提现", depositAddress: "你的充值地址", depositAddressHint: "同一个 EVM 兼容地址可用于所有已支持的网络。", depositQrHint: "扫描二维码获取充值地址", copy: "复制", copied: "地址已复制", noAddress: "专属 EVM 地址正在准备，请稍后刷新。",
    noActivity: "暂无流水", noActivityBody: "确认后的充值、转账和提现会以不可变的 USD 记录显示在这里。", amount: "金额", status: "状态", asset: "资产", blockchain: "区块链", transaction: "交易", action: "动作", user: "用户", time: "时间", reference: "参考号", details: "详情", viewOnExplorer: "在区块浏览器中查看",
    depositTitle: "充值 USDC 或 USDT", depositBody: "将任一支持的 USDC 或 USDT 转入你的专属地址。Midas 会自动发现交易，并在通过最终链上回执验证后才记入 USD 余额。", depositDiscoveryDelay: "后台会自动发现充值。按当前账户数量，通常约 30 秒内显示。", supportedAssets: "支持的资产", network: "网络", transactionHash: "交易哈希", confirmDeposit: "确认充值", depositNotReceived: "充值没有到账？", claimDepositTitle: "补录充值", claimDepositBody: "选择网络并填入已经完成的交易哈希。Midas 会从最终链上回执推导代币和金额，通过验证后才计入 USD 余额。", claimDepositHint: "只需要网络和交易哈希；同一笔交易只能入账一次。", claimDeposit: "核验并入账", depositClaimed: "充值已入账",
    transferTitle: "转账 USD", transferBody: "按 Linkit 用户名检索收款人并原子地转入 USD；如有需要，Midas 会自动创建收款人的账户和钱包。", recipient: "收款人", recipientHint: "搜索 Linkit 用户名；选中的收款人会自动获得 Midas 账户和专属钱包。", usdAmount: "USD 金额", sendTransfer: "发送转账", profile: "Linkit 资料", unknownProfile: "未找到 Linkit 资料；请从 Linkit 目录中选择收款人。",
    withdrawTitle: "提现稳定币", withdrawBody: "先选择网络和代币，再输入接收提现的 EVM 地址。", destination: "目标地址", requestWithdrawal: "请求提现", finalize: "最终确认", noWithdrawals: "暂无提现", awaitingSigner: "等待托管钱包签名", submitted: "已提交", completed: "已完成", failed: "失败", addressBook: "已保存的提现目标地址", addressBookBody: "在使用提现后，可从提现记录中保存并标记地址。", addressLabel: "地址标签", addressAdded: "目标地址已保存", renameAddress: "重命名", saveAddress: "保存", saveDestination: "保存目标地址", removeAddress: "删除", savedDestination: "已保存目标地址", withdrawalAddressBook: "提现地址簿", withdrawalAddressBookBody: "Midas 广播过的提现目标会显示在这里；可按网络和稳定币组合保存备注。", noWithdrawalTargets: "暂无提现目标地址", noWithdrawalTargetsBody: "先完成一次已广播的提现，随后可在这里保存对应的网络、代币和地址。", withdrawalTargetNote: "备注", saveNote: "保存备注", editNote: "编辑备注", removeNote: "删除备注", withdrawalTargetPicker: "已保存的目标地址", selectWithdrawalTarget: "选择历史提现", withdrawalTargetHint: "选择后会自动填入网络、代币和地址。",
    automaticReceipts: "自动收款", automaticReceiptsBody: "为外部应用创建收款渠道。只有明确授权该渠道的用户，才会被对应 API Key 自动扣费。", newPaymentChannel: "新建收款渠道", channelName: "渠道名称", createChannel: "创建渠道", noPaymentChannels: "暂无收款渠道", apiKey: "API Key", apiKeyCreated: "请立即复制 API Key", apiKeyCreatedBody: "为保障安全，Midas 不会再次展示这个 Key。外部扣费需要该 Key 和 Idempotency-Key 请求头。", copyApiKey: "复制 API Key", channelKeyPrefix: "Key 前缀", authorizationLink: "授权链接", automaticPayments: "自动付费", automaticPaymentsBody: "这些渠道可使用自己的 API Key 从你的可用 USD 余额自动扣费，你可以随时解除授权。", noAutomaticPayments: "暂无自动付费渠道", noAutomaticPaymentsBody: "授权一个自动收款渠道后，它会显示在这里。", unbind: "解除授权", boundOn: "已授权", signAgreement: "授权自动付款", signAgreementBody: "确认允许该渠道使用其 API Key 从你的 Midas 可用 USD 余额自动扣费。", authorizeAutomaticCharge: "确认授权渠道", agreementAuthorized: "已授权自动付款", agreementAlreadyAuthorized: "这个收款渠道已经获得授权。", invalidAgreementLink: "这个授权链接无效，或收款渠道已不存在。", channelOwner: "渠道所有者",
    setupTitle: "初始化 Midas", setupBody: "第一位认证用户将成为根管理员。该操作只能执行一次。", initialize: "初始化为根管理员", initialized: "Midas 已初始化", rootOnly: "只有根管理员可以修改 EVM 托管配置。",
    rootConfig: "托管钱包", rootConfigBody: "Midas 已内置 Ethereum、BNB Smart Chain、Base、Arbitrum、OP Mainnet、Polygon 的 USDC/USDT 映射与已验证 RPC 端点。只需输入一个私钥，Midas 会推导地址并用于 Gas、归集和提现。", custodyPrivateKey: "托管钱包私钥", custodyAddress: "托管钱包地址", saveConfiguration: "保存配置", secretsConfigured: "托管钱包已配置", noNetworks: "暂无支持的 EVM 网络。", custodyBalances: "托管余额", custodyBalancesBody: "查看托管钱包在每条已支持 EVM 链上的原生代币、USDC 与 USDT 实时余额。", noCustodyBalances: "请先配置托管钱包", noCustodyBalancesBody: "Midas 推导并保存托管钱包地址后即可读取余额。", nativeToken: "原生代币", unavailable: "暂不可用",
    rpcDiscovery: "RPC 充值发现", rpcDiscoveryBody: "Midas 每秒通过已配置的链 RPC 轮询一个「充值地址 × 网络」组合。每笔候选 Transfer 都会再次使用最终交易回执验证后才入账；不需要 Etherscan API Key。", discoveryPolling: "轮询间隔", discoveryLastAttempt: "最近尝试", discoveryLastSuccess: "最近成功轮询", discoveryLastError: "最近发现错误", discoveryNotStarted: "充值发现尚未轮询任何地址。", seconds: "秒",
    collectionOperations: "归集流水", collectionOperationsBody: "查看全部已入账充值，以及 Gas 充值和代币归集状态。只有尚未提交的记录可以重新归集。", noCollectionOperations: "暂无归集流水", noCollectionOperationsBody: "已入账充值会在这里显示归集状态。", collectionUser: "用户 ID", collectionGasTransaction: "Gas 交易", collectionTokenTransaction: "归集交易", collectionError: "最近错误", retryCollection: "重新归集", retryCollectionTitle: "重新归集这笔充值？", retryCollectionBody: "Midas 会向充值地址补充 Gas，并提交该地址的代币归集交易。请先核对当前状态与交易参考号。", retryCollectionNotice: "此操作会发送链上交易，无法撤销。", collectionRetryStarted: "已提交重新归集", collectionQueued: "待归集", collectionAwaitingConfiguration: "等待配置", collectionSubmitted: "已提交", collectionFailed: "失败", collectionSwept: "已归集",
    withdrawalOperations: "提现流水", withdrawalOperationsBody: "查看每笔提现及其链上状态。存在 TxID 时，重试只会重新广播同一笔已保存交易；只有不存在 TxID 时才会重新构造交易。", noWithdrawalOperations: "暂无提现记录", noWithdrawalOperationsBody: "用户提交提现后会立即在这里出现。", withdrawalDestination: "目标地址", withdrawalError: "最近错误", retryWithdrawal: "重试提现", retryWithdrawalTitle: "重试这笔提现吗？", retryWithdrawalBody: "Midas 会重新广播同一笔已保存交易；只有不存在 TxID 时，才会构造一笔新的交易。", retryWithdrawalNotice: "已有 TxID 的提现不会通过此操作重新签名。", withdrawalRetryStarted: "已提交提现重试",
    userBalancesTitle: "用户余额", userBalancesBody: "查看全部 Midas 账户及其当前可用 USD 余额。", totalHeld: "沉淀总余额", fundedAccounts: "有余额账户", allAccounts: "全部账户", noUsers: "暂无 Midas 账户", globalLedgerTitle: "全局流水", globalLedgerBody: "查看全部不可变 Midas 流水；充值和提现会在有链上交易时显示链上信息。", allKinds: "全部动作", allStatuses: "全部状态", transferIn: "转入", transferOut: "转出", adjustment: "调账", pending: "待处理", posted: "已入账", rejected: "已拒绝", disabled: "已停用", filter: "应用筛选", userIdFilter: "用户 ID", userIdFilterHint: "精确的 Auth Mini UUID", totalEntries: "流水总数", previous: "上一页", next: "下一页", page: "页", noLedgerEntries: "没有符合这些筛选条件的流水。",
    loading: "正在加载账户…", requestFailed: "请求失败", unknownRequestError: "Midas 未能完成请求，请稍后重试。", evmRpcUnavailable: "Midas 暂时无法从区块链读取这笔交易。余额未变更，请稍后重试。", transactionNotConfirmed: "这笔交易尚未在链上确认，请稍后重试。", transactionReverted: "这笔交易已在链上回滚，无法入账。", depositTransferMissing: "未找到发送到当前充值地址的所选代币转账。请检查网络、代币和交易哈希。", depositAlreadyCredited: "这笔链上转账已经入账，不能重复充值。", openApi: "开放 API", security: "使用 Auth Mini 认证 · 使用 Linkit 身份资料", confirm: "确认", cancel: "取消", refresh: "刷新", wallet: "钱包",
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
      <AuthMiniProvider authMiniBaseUrl={config.auth_mini_base_url} audiences={config.audiences} autoRedirectToLogin>
        <HashRouter><App linkitBaseUrl={config.linkit_base_url} /></HashRouter>
        <GlobalToaster />
      </AuthMiniProvider>
    </QueryClientProvider>
  </ThemeProvider>
}

function GlobalToaster() {
  return createPortal(<Toaster position="top-center" closeButton />, document.body)
}

function App({ linkitBaseUrl }: { linkitBaseUrl: string }) {
  const [language, setLanguage] = useState<Language>(() => window.localStorage.getItem("midas-language") === "zh" ? "zh" : "en")
  const [navigationOpen, setNavigationOpen] = useState(false)
  const t = (key: keyof typeof messages.en) => messages[language][key]
  const auth = useAuthMini()
  const location = useLocation()
  const setup = useQuery({ queryKey: ["setup"], queryFn: () => publicRequest<SetupStatus>("/api/setup/status") })
  const currentUserId = subjectFromToken(auth.session?.accessToken ?? undefined)
  const root = Boolean(setup.data?.initialized && setup.data.root_user_id === currentUserId)

  useEffect(() => { window.localStorage.setItem("midas-language", language) }, [language])

  return <LinkitProvider lang={language === "zh" ? "zh-CN" : "en"} linkitBaseUrl={linkitBaseUrl}><div className="min-h-dvh bg-background">
    <header className="sticky top-0 border-b bg-background">
      <div className="mx-auto flex max-w-5xl items-center justify-between gap-3 px-4 py-3 sm:px-6">
        <div className="flex min-w-0 items-center gap-2"><Button aria-label={t("menu")} size="icon" variant="ghost" onClick={() => setNavigationOpen(true)}><MenuIcon /></Button><Link to="/" className="flex min-w-0 items-center gap-2 font-medium"><LandmarkIcon aria-hidden="true" /> <span className="truncate">{t("appName")}</span><Badge variant="secondary">USD</Badge></Link></div>
        <div className="flex shrink-0 items-center gap-1">
          <LanguageMenu language={language} setLanguage={setLanguage} t={t} />
          <LinkitAppHeaderUser className="inline-flex max-w-24 min-w-0 shrink-0 items-center gap-2 overflow-hidden whitespace-nowrap sm:max-w-44 [&>.linkit-app-header-user__name]:truncate" lang={language === "zh" ? "zh-CN" : "en"} />
        </div>
      </div>
    </header>
    <main id="main-content" className="mx-auto w-full max-w-5xl px-4 py-6 sm:px-6 sm:py-8">
      <Routes>
        <Route path="/" element={<Dashboard t={t} language={language} />} />
        <Route path="/activity" element={<ActivityPage t={t} />} />
        <Route path="/withdrawal-address-book" element={<WithdrawalAddressBookPage t={t} />} />
        <Route path="/automatic-receipts" element={<AutomaticReceiptsPage t={t} />} />
        <Route path="/automatic-payments" element={<AutomaticPaymentsPage t={t} />} />
        <Route path="/sign_agreement" element={<SignAgreementPage t={t} />} />
        <Route path="/settings" element={<SettingsPage t={t} setup={setup.data} currentUserId={currentUserId} />} />
        <Route path="/admin/custody" element={<AdminRoute t={t} root={root}><RootConfigPage t={t} /></AdminRoute>} />
        <Route path="/admin/deposit-discovery" element={<AdminRoute t={t} root={root}><RootDepositDiscoveryPage t={t} /></AdminRoute>} />
        <Route path="/admin/collections" element={<AdminRoute t={t} root={root}><RootCollectionsPage t={t} language={language} /></AdminRoute>} />
        <Route path="/admin/withdrawals" element={<AdminRoute t={t} root={root}><RootWithdrawalsPage t={t} language={language} /></AdminRoute>} />
        <Route path="/admin/balances" element={<AdminRoute t={t} root={root}><AdminBalancesPage t={t} language={language} /></AdminRoute>} />
        <Route path="/admin/ledger" element={<AdminRoute t={t} root={root}><AdminLedgerPage t={t} language={language} /></AdminRoute>} />
        <Route path="*" element={<Dashboard t={t} language={language} />} />
      </Routes>
    </main>
    {auth.isAuthenticated && <NavigationDrawer currentPath={location.pathname} t={t} root={root} open={navigationOpen} setOpen={setNavigationOpen} />}
  </div></LinkitProvider>
}

function Dashboard({ t, language }: { t: Translate; language: Language }) {
  const api = useApi()
  const [drawer, setDrawer] = useState<"transfer" | "withdraw" | null>(null)
  const balance = useQuery({ queryKey: ["balance"], queryFn: () => api<Balance>("/api/balances/me") })
  const wallets = useQuery({ queryKey: ["wallets"], queryFn: () => api<WalletAddress[]>("/api/wallet-addresses/me") })
  const assets = useQuery({ queryKey: ["assets"], queryFn: () => api<Asset[]>("/api/assets") })
  const ledger = useQuery({ queryKey: ["ledger"], queryFn: () => api<LedgerEntry[]>("/api/ledger/me") })

  if (balance.isLoading || wallets.isLoading || assets.isLoading) return <LoadingScreen t={t} />
  if (balance.error || wallets.error || assets.error) return <RequestError t={t} error={balance.error ?? wallets.error ?? assets.error} />

  return <div className="flex flex-col gap-6">
    <section className="flex flex-col gap-3"><div className="flex flex-wrap items-center justify-between gap-3"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("balance")}</h1></div><Badge variant="secondary">{t("usdOnly")}</Badge></div>
      <Card><CardHeader><CardTitle>{t("balance")}</CardTitle><CardDescription>{t("usdOnly")}</CardDescription><CardAction><WalletCardsIcon aria-hidden="true" /></CardAction></CardHeader><CardContent><p className="text-3xl font-semibold tracking-tight tabular-nums">${formatUsd(balance.data?.available_usd)}</p></CardContent><CardFooter className="grid grid-cols-2 gap-2 lg:flex"><Button variant="outline" onClick={() => setDrawer("transfer")}><ArrowLeftRightIcon data-icon="inline-start" />{t("transfer")}</Button><Button variant="outline" onClick={() => setDrawer("withdraw")}><ArrowUpFromLineIcon data-icon="inline-start" />{t("withdraw")}</Button></CardFooter></Card>
    </section>
    <section className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)]">
      <DepositAddressCard t={t} wallets={wallets.data ?? []} assets={assets.data ?? []} />
      <ActivityPreview t={t} entries={ledger.data ?? []} />
    </section>
    <TransferDrawer open={drawer === "transfer"} setOpen={(open) => !open && setDrawer(null)} t={t} language={language} />
    <WithdrawalDrawer open={drawer === "withdraw"} setOpen={(open) => !open && setDrawer(null)} t={t} assets={assets.data ?? []} />
  </div>
}

function DepositAddressCard({ t, wallets, assets }: { t: Translate; wallets: WalletAddress[]; assets: Asset[] }) {
  const address = wallets[0]
  const supportedAssets = assets.filter((asset) => asset.enabled)

  return <Card><CardHeader><CardTitle>{t("depositAddress")}</CardTitle><CardDescription>{t("depositAddressHint")}</CardDescription></CardHeader><CardContent>{address ? <div className="flex flex-col gap-4"><div className="flex items-center gap-2"><code className="min-w-0 flex-1 truncate rounded-md bg-muted px-3 py-2 text-sm">{address.address}</code><Button aria-label={t("copy")} size="icon" variant="outline" onClick={() => void navigator.clipboard.writeText(address.address).then(() => toast.success(t("copied")))}><ClipboardCopyIcon /></Button></div><div className="flex flex-col items-center gap-2"><QRCodeSVG value={address.address} size={176} level="M" marginSize={4} title={t("depositAddress")} /><span className="text-sm text-muted-foreground">{t("depositQrHint")}</span></div></div> : <Alert><AlertTitle>{t("noAddress")}</AlertTitle><AlertDescription>{t("refresh")}</AlertDescription></Alert>}</CardContent><CardFooter className="flex flex-col items-start gap-2"><span className="text-sm font-medium">{t("supportedAssets")}</span><div className="flex flex-wrap gap-2">{supportedAssets.map((asset) => <Badge key={asset.id} variant="outline">{asset.network_name ?? `Chain ${asset.chain_id}`} · {asset.symbol}</Badge>)}</div></CardFooter></Card>
}

function ActivityPreview({ t, entries }: { t: Translate; entries: LedgerEntry[] }) {
  return <Card><CardHeader><CardTitle>{t("activity")}</CardTitle><CardDescription>{t("details")}</CardDescription><CardAction><Button size="sm" variant="ghost" render={<Link to="/activity" />} nativeButton={false}><HistoryIcon data-icon="inline-start" />{t("activity")}</Button></CardAction></CardHeader><CardContent>{entries.length ? <LedgerTable t={t} entries={entries.slice(0, 5)} compact /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><HistoryIcon /></EmptyMedia><EmptyTitle>{t("noActivity")}</EmptyTitle><EmptyDescription>{t("noActivityBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card>
}

function ActivityPage({ t }: { t: Translate }) {
  const api = useApi()
  const ledger = useQuery({ queryKey: ["ledger"], queryFn: () => api<LedgerEntry[]>("/api/ledger/me") })
  const assets = useQuery({ queryKey: ["assets"], queryFn: () => api<Asset[]>("/api/assets") })
  const [claimOpen, setClaimOpen] = useState(false)
  if (ledger.isLoading || assets.isLoading) return <LoadingScreen t={t} />
  if (ledger.error || assets.error) return <RequestError t={t} error={ledger.error ?? assets.error} />
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("activity")}</h1></div><Card><CardHeader><CardTitle>{t("activity")}</CardTitle><CardDescription>{t("noActivityBody")}</CardDescription></CardHeader><CardContent>{ledger.data?.length ? <LedgerTable t={t} entries={ledger.data} /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><HistoryIcon /></EmptyMedia><EmptyTitle>{t("noActivity")}</EmptyTitle><EmptyDescription>{t("noActivityBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent><CardFooter><Button size="sm" variant="link" onClick={() => setClaimOpen(true)}>{t("depositNotReceived")}</Button></CardFooter></Card><DepositClaimDrawer open={claimOpen} setOpen={setClaimOpen} t={t} assets={assets.data ?? []} /></section>
}

function LedgerTable({ t, entries, compact = false }: { t: Translate; entries: LedgerEntry[]; compact?: boolean }) {
  return <><div className="flex flex-col sm:hidden">{entries.map((entry, index) => <div key={entry.id}><div className="flex items-start justify-between gap-3 py-3"><div className="min-w-0 flex-1"><div className="flex flex-wrap items-center gap-2"><span className="font-medium capitalize">{ledgerActionLabel(entry.kind, t)}</span><StatusBadge t={t} status={entry.status} /></div><span className="mt-1 block truncate text-muted-foreground text-xs">{entry.note ?? entry.external_reference ?? entry.id}</span><ChainTransactionDetails entry={entry} t={t} /><span className="mt-1 text-muted-foreground text-xs">{formatTime(entry.created_at)}</span></div><span className="shrink-0 font-medium tabular-nums">{entry.balance_delta_usd_micros < 0 ? "−" : "+"}${formatMicros(Math.abs(entry.balance_delta_usd_micros))}</span></div>{index < entries.length - 1 ? <Separator /> : null}</div>)}</div><div className="hidden sm:block"><Table><TableHeader><TableRow><TableHead>{t("action")}</TableHead><TableHead>{t("blockchain")}</TableHead><TableHead>{t("status")}</TableHead><TableHead className="text-right">{t("amount")}</TableHead>{!compact && <TableHead>{t("time")}</TableHead>}</TableRow></TableHeader><TableBody>{entries.map((entry) => <TableRow key={entry.id}><TableCell><div className="flex flex-col gap-1"><span className="font-medium capitalize">{ledgerActionLabel(entry.kind, t)}</span><span className="max-w-40 truncate text-muted-foreground text-xs">{entry.note ?? entry.external_reference ?? entry.id}</span></div></TableCell><TableCell><ChainTransactionDetails entry={entry} t={t} /></TableCell><TableCell><StatusBadge t={t} status={entry.status} /></TableCell><TableCell className="text-right"><span className="font-medium tabular-nums">{entry.balance_delta_usd_micros < 0 ? "−" : "+"}${formatMicros(Math.abs(entry.balance_delta_usd_micros))}</span></TableCell>{!compact && <TableCell className="whitespace-nowrap"><span className="text-muted-foreground">{formatTime(entry.created_at)}</span></TableCell>}</TableRow>)}</TableBody></Table></div></>
}

function ChainTransactionDetails({ entry, t }: { entry: LedgerEntry; t: Translate }) {
  const url = transactionExplorerUrl(entry.chain_id, entry.transaction_hash)
  const transactionHash = entry.transaction_hash
  if (!entry.network_name || !entry.asset_symbol) return <span className="text-muted-foreground">—</span>
  return <div className="flex min-w-0 flex-wrap items-center gap-1.5"><Badge variant="outline">{entry.network_name}</Badge><Badge variant="outline">{entry.asset_symbol}</Badge>{url && transactionHash ? <a href={url} target="_blank" rel="noreferrer" aria-label={t("viewOnExplorer")} className="inline-flex min-w-0 items-center gap-1 text-muted-foreground hover:text-foreground"><code className="max-w-28 truncate text-xs">{shortTransactionHash(transactionHash)}</code><ExternalLinkIcon className="size-3" /></a> : <span className="text-muted-foreground">—</span>}</div>
}

function DepositDrawer({ open, setOpen, t, assets }: { open: boolean; setOpen: (open: boolean) => void; t: Translate; assets: Asset[] }) {
  const supportedAssets = assets.filter((asset) => asset.enabled)
  return <Drawer open={open} onOpenChange={setOpen}><DrawerContent><DrawerHeader><DrawerTitle>{t("depositTitle")}</DrawerTitle><DrawerDescription>{t("depositBody")}</DrawerDescription></DrawerHeader><div className="flex min-h-0 flex-1 flex-col"><div className="overflow-y-auto p-4"><FieldGroup><Field><FieldLabel>{t("supportedAssets")}</FieldLabel><div className="flex flex-wrap gap-2">{supportedAssets.map((asset) => <Badge key={asset.id} variant="outline">{asset.symbol} · {asset.network_name ?? `Chain ${asset.chain_id}`}</Badge>)}</div><FieldDescription>{t("depositDiscoveryDelay")}</FieldDescription></Field><Alert><ArrowDownToLineIcon /><AlertTitle>{t("deposit")}</AlertTitle><AlertDescription>{t("depositBody")}</AlertDescription></Alert></FieldGroup></div><DrawerFooter><Button type="button" onClick={() => setOpen(false)}>{t("confirm")}</Button></DrawerFooter></div></DrawerContent></Drawer>
}

function DepositClaimDrawer({ open, setOpen, t, assets }: { open: boolean; setOpen: (open: boolean) => void; t: Translate; assets: Asset[] }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [chainId, setChainId] = useState("")
  const [transactionHash, setTransactionHash] = useState("")
  const networks = enabledNetworks(assets)
  const networkItems = networks.map((network) => ({ value: String(network.chain_id), label: network.name }))
  const validTransactionHash = /^0x[a-fA-F0-9]{64}$/.test(transactionHash)
  const submit = useMutation({ mutationFn: () => api<Deposit>("/api/deposits/claim", { method: "POST", idempotency: crypto.randomUUID(), body: { chain_id: Number(chainId), transaction_hash: transactionHash } }), onSuccess: () => { toast.success(t("depositClaimed")); void queryClient.invalidateQueries({ queryKey: ["balance"] }); void queryClient.invalidateQueries({ queryKey: ["ledger"] }); setTransactionHash(""); setOpen(false) }, onError: (error) => showApiError(error, t) })
  return <Drawer open={open} onOpenChange={setOpen}><DrawerContent><DrawerHeader><DrawerTitle>{t("claimDepositTitle")}</DrawerTitle><DrawerDescription>{t("claimDepositBody")}</DrawerDescription></DrawerHeader><form onSubmit={(event) => { event.preventDefault(); submit.mutate() }} className="flex min-h-0 flex-1 flex-col"><div className="overflow-y-auto p-4"><FieldGroup><Field><FieldLabel>{t("network")}</FieldLabel><Select items={networkItems} value={chainId || null} onValueChange={(value) => setChainId(value ?? "")}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{networkItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field data-invalid={transactionHash.length > 0 && !validTransactionHash}><FieldLabel htmlFor="deposit-claim-transaction-hash">{t("transactionHash")}</FieldLabel><Input id="deposit-claim-transaction-hash" value={transactionHash} onChange={(event) => setTransactionHash(event.target.value.trim())} aria-invalid={transactionHash.length > 0 && !validTransactionHash} autoCapitalize="none" autoCorrect="off" placeholder="0x…" /><FieldDescription>{t("claimDepositHint")}</FieldDescription></Field></FieldGroup></div><DrawerFooter><Button type="submit" disabled={submit.isPending || !chainId || !validTransactionHash}>{submit.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("claimDeposit")}</Button><Button type="button" variant="outline" disabled={submit.isPending} onClick={() => setOpen(false)}>{t("cancel")}</Button></DrawerFooter></form></DrawerContent></Drawer>
}

function TransferDrawer({ open, setOpen, t, language }: { open: boolean; setOpen: (open: boolean) => void; t: Translate; language: Language }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [recipientUserId, setRecipientUserId] = useState("")
  const [amount, setAmount] = useState("")
  const [note, setNote] = useState("")
  const submit = useMutation({ mutationFn: () => api("/api/transfers", { method: "POST", idempotency: crypto.randomUUID(), body: { recipient_user_id: recipientUserId, amount_usd_micros: usdToMicros(amount), note: note || undefined } }), onSuccess: () => { toast.success(t("sendTransfer")); void queryClient.invalidateQueries({ queryKey: ["balance"] }); void queryClient.invalidateQueries({ queryKey: ["ledger"] }); setOpen(false) }, onError: (error) => showApiError(error, t) })
  const validAmount = usdToMicros(amount) > 0

  return <Drawer open={open} onOpenChange={setOpen}><DrawerContent><DrawerHeader><DrawerTitle>{t("transferTitle")}</DrawerTitle><DrawerDescription>{t("transferBody")}</DrawerDescription></DrawerHeader><form onSubmit={(event) => { event.preventDefault(); submit.mutate() }} className="flex min-h-0 flex-1 flex-col"><div className="overflow-y-auto p-4"><FieldGroup><Field><LinkitUserPicker name="recipient_user_id" value={recipientUserId} onValueChange={(userId) => setRecipientUserId(userId)} label={t("recipient")} lang={language === "zh" ? "zh-CN" : "en"} required /><FieldDescription>{t("recipientHint")}</FieldDescription></Field><UsdField t={t} amount={amount} setAmount={setAmount} /><Field><FieldLabel htmlFor="transfer-note">{t("details")}</FieldLabel><Input id="transfer-note" value={note} onChange={(event) => setNote(event.target.value)} maxLength={280} /></Field></FieldGroup></div><DrawerFooter><Button type="submit" disabled={submit.isPending || !recipientUserId || !validAmount}>{submit.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("sendTransfer")}</Button><Button type="button" variant="outline" onClick={() => setOpen(false)}>{t("cancel")}</Button></DrawerFooter></form></DrawerContent></Drawer>
}

function WithdrawalDrawer({ open, setOpen, t, assets }: { open: boolean; setOpen: (open: boolean) => void; t: Translate; assets: Asset[] }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [chainId, setChainId] = useState("")
  const [assetId, setAssetId] = useState("")
  const [destinationAddress, setDestinationAddress] = useState("")
  const [amount, setAmount] = useState("")
  const [selectedTargetId, setSelectedTargetId] = useState("")
  const targets = useQuery({ queryKey: ["withdrawal-targets"], queryFn: () => api<WithdrawalAddressTarget[]>("/api/withdrawal-targets/me"), enabled: open })
  const networks = enabledNetworks(assets)
  const networkItems = networks.map((network) => ({ value: String(network.chain_id), label: network.name }))
  const assetItems = assets.filter((asset) => asset.enabled && String(asset.chain_id) === chainId).map((asset) => ({ value: asset.id, label: asset.symbol }))
  const validDestination = /^0x[a-fA-F0-9]{40}$/.test(destinationAddress)
  const submit = useMutation({ mutationFn: () => api<Withdrawal>("/api/withdrawals", { method: "POST", idempotency: crypto.randomUUID(), body: { asset_id: assetId, destination_address: destinationAddress, amount_usd_micros: usdToMicros(amount) } }), onSuccess: () => { toast.success(t("requestWithdrawal")); void queryClient.invalidateQueries({ queryKey: ["balance"] }); void queryClient.invalidateQueries({ queryKey: ["withdrawals"] }); void queryClient.invalidateQueries({ queryKey: ["ledger"] }); setOpen(false) }, onError: (error) => showApiError(error, t) })
  const targetItems = (targets.data ?? []).map((target) => ({ value: target.asset_id + target.address, label: `${target.label ?? target.network_name} · ${target.network_name} · ${target.asset_symbol} · ${shortAddress(target.address)}` }))
  const chooseTarget = (value: string | null) => { const target = (targets.data ?? []).find((item) => item.asset_id + item.address === value); setSelectedTargetId(value ?? ""); if (target) { setChainId(String(target.chain_id)); setAssetId(target.asset_id); setDestinationAddress(target.address) } }
  const selectedTarget = (targets.data ?? []).find((target) => target.asset_id + target.address === selectedTargetId)
  return <Drawer open={open} onOpenChange={setOpen}><DrawerContent><DrawerHeader><DrawerTitle>{t("withdrawTitle")}</DrawerTitle><DrawerDescription>{t("withdrawBody")}</DrawerDescription></DrawerHeader><form onSubmit={(event) => { event.preventDefault(); submit.mutate() }} className="flex min-h-0 flex-1 flex-col"><div className="overflow-y-auto p-4"><FieldGroup><Field><FieldLabel>{t("withdrawalTargetPicker")}</FieldLabel><Select items={targetItems} value={selectedTargetId || null} onValueChange={chooseTarget} disabled={targets.isLoading || targetItems.length === 0}><SelectTrigger className="w-full"><SelectValue placeholder={t("selectWithdrawalTarget")} /></SelectTrigger><SelectContent><SelectGroup>{targetItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select><FieldDescription>{selectedTarget?.label ?? t("withdrawalTargetHint")}</FieldDescription></Field><Field><FieldLabel>{t("network")}</FieldLabel><Select items={networkItems} value={chainId || null} onValueChange={(value) => { setSelectedTargetId(""); setChainId(value ?? ""); setAssetId("") }}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{networkItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel>{t("asset")}</FieldLabel><Select items={assetItems} value={assetId || null} onValueChange={(value) => { setSelectedTargetId(""); setAssetId(value ?? "") }} disabled={!chainId}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{assetItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field data-invalid={destinationAddress.length > 0 && !validDestination}><FieldLabel htmlFor="withdrawal-destination">{t("destination")}</FieldLabel><Input id="withdrawal-destination" value={destinationAddress} onChange={(event) => { setSelectedTargetId(""); setDestinationAddress(event.target.value.trim()) }} aria-invalid={destinationAddress.length > 0 && !validDestination} placeholder="0x…" /></Field><UsdField t={t} amount={amount} setAmount={setAmount} /></FieldGroup></div><DrawerFooter><Button type="submit" disabled={submit.isPending || !chainId || !assetId || !validDestination || usdToMicros(amount) <= 0}>{submit.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("requestWithdrawal")}</Button><Button type="button" variant="outline" onClick={() => setOpen(false)}>{t("cancel")}</Button></DrawerFooter></form></DrawerContent></Drawer>
}

function UsdField({ t, amount, setAmount }: { t: Translate; amount: string; setAmount: (value: string) => void }) {
  const invalid = amount.length > 0 && usdToMicros(amount) <= 0
  return <Field data-invalid={invalid}><FieldLabel htmlFor="usd-amount">{t("usdAmount")}</FieldLabel><Input id="usd-amount" inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} aria-invalid={invalid} placeholder="0.000000" /><FieldDescription>USD · 6 decimals</FieldDescription></Field>
}

function SettingsPage({ t, setup, currentUserId }: { t: Translate; setup?: SetupStatus; currentUserId: string | null }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const initialize = useMutation({ mutationFn: () => api<SetupStatus>("/api/setup/initialize", { method: "POST", body: { root_user_id: currentUserId } }), onSuccess: () => { toast.success(t("initialized")); void queryClient.invalidateQueries({ queryKey: ["setup"] }) }, onError: (error) => showApiError(error, t) })
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("settings")}</h1></div>
    {!setup?.initialized ? <Card><CardHeader><CardTitle>{t("setupTitle")}</CardTitle><CardDescription>{t("setupBody")}</CardDescription></CardHeader><CardFooter><Button disabled={!currentUserId || initialize.isPending} onClick={() => initialize.mutate()}>{initialize.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("initialize")}</Button></CardFooter></Card> : null}
  </section>
}

function WithdrawalAddressBookPage({ t }: { t: Translate }) {
  const api = useApi()
  const targets = useQuery({ queryKey: ["withdrawal-targets"], queryFn: () => api<WithdrawalAddressTarget[]>("/api/withdrawal-targets/me") })
  if (targets.isLoading) return <LoadingScreen t={t} />
  if (targets.error) return <RequestError t={t} error={targets.error} />
  const rows = targets.data ?? []
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("withdrawalAddressBook")}</h1></div><Card><CardHeader><CardTitle>{t("withdrawalAddressBook")}</CardTitle><CardDescription>{t("withdrawalAddressBookBody")}</CardDescription></CardHeader><CardContent>{rows.length ? <div className="flex flex-col">{rows.map((target, index) => <div key={`${target.asset_id}:${target.address}`}><WithdrawalAddressTargetRow t={t} target={target} />{index < rows.length - 1 ? <Separator /> : null}</div>)}</div> : <Empty><EmptyHeader><EmptyMedia variant="icon"><ArrowUpFromLineIcon /></EmptyMedia><EmptyTitle>{t("noWithdrawalTargets")}</EmptyTitle><EmptyDescription>{t("noWithdrawalTargetsBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card></section>
}

function WithdrawalAddressTargetRow({ t, target }: { t: Translate; target: WithdrawalAddressTarget }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState(!target.label)
  const [label, setLabel] = useState(target.label ?? "")
  const save = useMutation({ mutationFn: () => api<WithdrawalAddressTarget>("/api/withdrawal-targets/me", { method: "PUT", body: { asset_id: target.asset_id, address: target.address, label } }), onSuccess: () => { toast.success(t("saveNote")); setEditing(false); void queryClient.invalidateQueries({ queryKey: ["withdrawal-targets"] }) }, onError: (error) => showApiError(error, t) })
  const remove = useMutation({ mutationFn: () => api<void>(`/api/withdrawal-targets/me/${target.note_id}`, { method: "DELETE" }), onSuccess: () => { setLabel(""); setEditing(true); void queryClient.invalidateQueries({ queryKey: ["withdrawal-targets"] }) }, onError: (error) => showApiError(error, t) })
  return <div className="flex flex-col gap-3 py-4"><div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2">{target.label ? <span className="font-medium">{target.label}</span> : <span className="font-medium">{t("withdrawalTargetNote")}</span>}<Badge variant="outline">{target.network_name}</Badge><Badge variant="outline">{target.asset_symbol}</Badge></div><code className="mt-1 block max-w-full truncate text-muted-foreground text-xs">{target.address}</code><span className="mt-1 block text-muted-foreground text-xs">{formatTime(target.last_withdrawn_at)}</span></div>{!editing ? <div className="flex items-center gap-1"><Button size="sm" variant="ghost" onClick={() => setEditing(true)}>{t("editNote")}</Button>{target.note_id ? <Button size="sm" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate()}>{t("removeNote")}</Button> : null}</div> : null}</div>{editing ? <form onSubmit={(event) => { event.preventDefault(); save.mutate() }}><FieldGroup className="flex flex-col gap-2 sm:flex-row sm:items-end"><Field><FieldLabel htmlFor={`withdrawal-target-note-${target.asset_id}-${target.address}`}>{t("withdrawalTargetNote")}</FieldLabel><Input id={`withdrawal-target-note-${target.asset_id}-${target.address}`} value={label} onChange={(event) => setLabel(event.target.value)} maxLength={80} /></Field><div className="flex gap-2"><Button size="sm" type="submit" disabled={save.isPending || !label.trim()}>{save.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("saveNote")}</Button>{target.note_id ? <Button size="sm" type="button" variant="outline" onClick={() => { setLabel(target.label ?? ""); setEditing(false) }}>{t("cancel")}</Button> : null}</div></FieldGroup></form> : null}</div>
}

function AutomaticReceiptsPage({ t }: { t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [name, setName] = useState("")
  const [created, setCreated] = useState<PaymentAgreementCreated | null>(null)
  const agreements = useQuery({ queryKey: ["owned-agreements"], queryFn: () => api<PaymentAgreement[]>("/api/agreements/owned") })
  const create = useMutation({ mutationFn: () => api<PaymentAgreementCreated>("/api/agreements/owned", { method: "POST", body: { name } }), onSuccess: (response) => { setName(""); setCreated(response); toast.success(t("createChannel")); void queryClient.invalidateQueries({ queryKey: ["owned-agreements"] }) }, onError: (error) => showApiError(error, t) })
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("automaticReceipts")}</h1></div><Card><CardHeader><CardTitle>{t("newPaymentChannel")}</CardTitle><CardDescription>{t("automaticReceiptsBody")}</CardDescription></CardHeader><CardContent><form onSubmit={(event) => { event.preventDefault(); create.mutate() }}><FieldGroup><Field><FieldLabel htmlFor="payment-channel-name">{t("channelName")}</FieldLabel><Input id="payment-channel-name" value={name} onChange={(event) => setName(event.target.value)} maxLength={80} /></Field><Button type="submit" disabled={create.isPending || !name.trim()}>{create.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("createChannel")}</Button></FieldGroup></form>{created ? <Alert className="mt-4"><AlertTitle>{t("apiKeyCreated")}</AlertTitle><AlertDescription>{t("apiKeyCreatedBody")}</AlertDescription><FieldGroup className="mt-3 flex flex-col gap-2 sm:flex-row sm:items-end"><Field><FieldLabel htmlFor="created-payment-api-key">{t("apiKey")}</FieldLabel><Input id="created-payment-api-key" value={created.api_key} readOnly /></Field><Button type="button" variant="outline" onClick={() => void navigator.clipboard.writeText(created.api_key).then(() => toast.success(t("copied")))}><ClipboardCopyIcon data-icon="inline-start" />{t("copyApiKey")}</Button></FieldGroup></Alert> : null}</CardContent></Card><Card><CardHeader><CardTitle>{t("automaticReceipts")}</CardTitle><CardDescription>{t("automaticReceiptsBody")}</CardDescription></CardHeader><CardContent>{agreements.isLoading ? <Skeleton className="h-24 w-full" /> : agreements.error ? <RequestError t={t} error={agreements.error} /> : agreements.data?.length ? <div className="flex flex-col">{agreements.data.map((agreement, index) => <div key={agreement.id}><PaymentAgreementRow agreement={agreement} t={t} />{index < agreements.data.length - 1 ? <Separator /> : null}</div>)}</div> : <Empty><EmptyHeader><EmptyMedia variant="icon"><ArrowDownToLineIcon /></EmptyMedia><EmptyTitle>{t("noPaymentChannels")}</EmptyTitle><EmptyDescription>{t("automaticReceiptsBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card></section>
}

function PaymentAgreementRow({ agreement, t }: { agreement: PaymentAgreement; t: Translate }) {
  const authorizationUrl = agreementAuthorizationUrl(agreement.id)
  return <div className="flex flex-col gap-2 py-3"><div className="flex flex-wrap items-center gap-2"><span className="font-medium">{agreement.name}</span><Badge variant="outline">{t("channelKeyPrefix")}: {agreement.api_key_prefix}</Badge></div><div className="flex min-w-0 flex-wrap items-center gap-2"><span className="text-muted-foreground text-xs">{t("authorizationLink")}</span><code className="min-w-0 flex-1 truncate text-xs">{authorizationUrl}</code><Button aria-label={t("copy")} size="icon" variant="ghost" onClick={() => void navigator.clipboard.writeText(authorizationUrl).then(() => toast.success(t("copied")))}><ClipboardCopyIcon /></Button></div></div>
}

function AutomaticPaymentsPage({ t }: { t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const bindings = useQuery({ queryKey: ["agreement-bindings"], queryFn: () => api<PaymentAgreementBinding[]>("/api/agreements/bindings/me") })
  const unbind = useMutation({ mutationFn: (agreementId: string) => api<void>(`/api/agreements/${agreementId}/bind`, { method: "DELETE" }), onSuccess: () => { toast.success(t("unbind")); void queryClient.invalidateQueries({ queryKey: ["agreement-bindings"] }) }, onError: (error) => showApiError(error, t) })
  if (bindings.isLoading) return <LoadingScreen t={t} />
  if (bindings.error) return <RequestError t={t} error={bindings.error} />
  const rows = bindings.data ?? []
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("automaticPayments")}</h1></div><Card><CardHeader><CardTitle>{t("automaticPayments")}</CardTitle><CardDescription>{t("automaticPaymentsBody")}</CardDescription></CardHeader><CardContent>{rows.length ? <div className="flex flex-col">{rows.map((binding, index) => <div key={binding.agreement.id}><div className="flex flex-wrap items-center justify-between gap-3 py-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="font-medium">{binding.agreement.name}</span><Badge variant="outline">{t("boundOn")}</Badge></div><code className="mt-1 block max-w-72 truncate text-muted-foreground text-xs">{binding.agreement.id}</code><span className="mt-1 block text-muted-foreground text-xs">{formatTime(binding.created_at)}</span></div><Button size="sm" variant="outline" disabled={unbind.isPending} onClick={() => unbind.mutate(binding.agreement.id)}>{unbind.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("unbind")}</Button></div>{index < rows.length - 1 ? <Separator /> : null}</div>)}</div> : <Empty><EmptyHeader><EmptyMedia variant="icon"><ArrowUpFromLineIcon /></EmptyMedia><EmptyTitle>{t("noAutomaticPayments")}</EmptyTitle><EmptyDescription>{t("noAutomaticPaymentsBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card></section>
}

function SignAgreementPage({ t }: { t: Translate }) {
  const auth = useAuthMini()
  const api = useApi()
  const queryClient = useQueryClient()
  const agreementId = new URLSearchParams(useLocation().search).get("agreement_id")
  const agreement = useQuery({ queryKey: ["agreement", agreementId], queryFn: () => api<PaymentAgreementDetail>(`/api/agreements/${agreementId}`), enabled: Boolean(auth.isAuthenticated && agreementId) })
  const authorize = useMutation({ mutationFn: () => api<PaymentAgreementBinding>(`/api/agreements/${agreementId}/bind`, { method: "POST" }), onSuccess: () => { toast.success(t("agreementAuthorized")); void queryClient.invalidateQueries({ queryKey: ["agreement", agreementId] }); void queryClient.invalidateQueries({ queryKey: ["agreement-bindings"] }) }, onError: (error) => showApiError(error, t) })
  if (!agreementId) return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("signAgreement")}</h1></div><Alert variant="destructive"><AlertTitle>{t("invalidAgreementLink")}</AlertTitle></Alert></section>
  if (!auth.isAuthenticated || agreement.isLoading) return <LoadingScreen t={t} />
  if (agreement.error || !agreement.data) return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("signAgreement")}</h1></div><Alert variant="destructive"><AlertTitle>{t("invalidAgreementLink")}</AlertTitle></Alert></section>
  const detail = agreement.data
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("wallet")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("signAgreement")}</h1></div><Card><CardHeader><CardTitle>{detail.agreement.name}</CardTitle><CardDescription>{t("signAgreementBody")}</CardDescription></CardHeader><CardContent><dl className="flex flex-col gap-3"><div className="flex flex-col gap-1"><dt className="text-sm text-muted-foreground">{t("channelOwner")}</dt><dd><code className="break-all text-sm">{detail.agreement.owner_user_id}</code></dd></div><div className="flex flex-col gap-1"><dt className="text-sm text-muted-foreground">{t("channelName")}</dt><dd className="font-medium">{detail.agreement.name}</dd></div></dl></CardContent><CardFooter>{detail.bound ? <Badge variant="secondary">{t("agreementAlreadyAuthorized")}</Badge> : <Button disabled={authorize.isPending} onClick={() => authorize.mutate()}>{authorize.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("authorizeAutomaticCharge")}</Button>}</CardFooter></Card></section>
}

function RootConfig({ t }: { t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const config = useQuery({ queryKey: ["evm-config"], queryFn: () => api<EvmConfig>("/api/admin/evm-config") })
  const balances = useQuery({ queryKey: ["custody-balances"], queryFn: () => api<CustodyBalances>("/api/admin/custody-balances"), enabled: Boolean(config.data?.custody_wallet_address) })
  const [privateKey, setPrivateKey] = useState("")
  const save = useMutation({ mutationFn: () => api<EvmConfig>("/api/admin/evm-config", { method: "PUT", body: { custody_wallet_private_key: privateKey || undefined } }), onSuccess: () => { toast.success(t("saveConfiguration")); setPrivateKey(""); void queryClient.invalidateQueries({ queryKey: ["evm-config"] }); void queryClient.invalidateQueries({ queryKey: ["custody-balances"] }) }, onError: (error) => showApiError(error, t) })
  if (config.isLoading) return <Skeleton className="h-60 w-full" />
  if (config.error) return <RequestError t={t} error={config.error} />
  return <><Card><CardHeader><CardTitle>{t("rootConfig")}</CardTitle><CardDescription>{t("rootConfigBody")}</CardDescription><CardAction>{config.data?.custody_wallet_private_key_configured ? <Badge variant="secondary">{t("secretsConfigured")}</Badge> : null}</CardAction></CardHeader><CardContent><form onSubmit={(event) => { event.preventDefault(); save.mutate() }}><FieldGroup>{config.data?.custody_wallet_address ? <Field><FieldLabel>{t("custodyAddress")}</FieldLabel><code className="truncate rounded-md bg-muted px-3 py-2 text-sm">{config.data.custody_wallet_address}</code></Field> : null}<Field><FieldLabel htmlFor="custody-private-key">{t("custodyPrivateKey")}</FieldLabel><Input id="custody-private-key" type="password" autoComplete="off" value={privateKey} onChange={(event) => setPrivateKey(event.target.value)} /><FieldDescription>{t("rootConfigBody")}</FieldDescription></Field><Button type="submit" disabled={save.isPending || !privateKey}>{save.isPending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("saveConfiguration")}</Button></FieldGroup></form></CardContent></Card><CustodyBalancesCard t={t} configured={Boolean(config.data?.custody_wallet_address)} balances={balances} /></>
}

function CustodyBalancesCard({ t, configured, balances }: { t: Translate; configured: boolean; balances: UseQueryResult<CustodyBalances, Error> }) {
  const data = balances.data
  return <Card><CardHeader><CardTitle>{t("custodyBalances")}</CardTitle><CardDescription>{t("custodyBalancesBody")}</CardDescription><CardAction>{configured ? <Button aria-label={t("refresh")} size="icon" variant="ghost" disabled={balances.isFetching} onClick={() => void balances.refetch()}><RefreshCwIcon data-icon="inline-start" /></Button> : null}</CardAction></CardHeader><CardContent>{!configured ? <Empty><EmptyHeader><EmptyMedia variant="icon"><WalletCardsIcon /></EmptyMedia><EmptyTitle>{t("noCustodyBalances")}</EmptyTitle><EmptyDescription>{t("noCustodyBalancesBody")}</EmptyDescription></EmptyHeader></Empty> : balances.isLoading ? <Skeleton className="h-64 w-full" /> : balances.error ? <RequestError t={t} error={balances.error} /> : data?.networks.length ? <CustodyBalanceTable t={t} entries={data.networks} /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><WalletCardsIcon /></EmptyMedia><EmptyTitle>{t("noCustodyBalances")}</EmptyTitle><EmptyDescription>{t("noCustodyBalancesBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card>
}

function CustodyBalanceTable({ t, entries }: { t: Translate; entries: CustodyNetworkBalances[] }) {
  return <><div className="flex flex-col sm:hidden">{entries.map((entry, index) => <div key={entry.chain_id}><div className="flex flex-col gap-3 py-3"><span className="font-medium">{entry.network_name}</span><dl className="grid grid-cols-3 gap-3"><CustodyBalanceItem t={t} label={entry.native.symbol} balance={entry.native} /><CustodyBalanceItem t={t} label={entry.usdc.symbol} balance={entry.usdc} /><CustodyBalanceItem t={t} label={entry.usdt.symbol} balance={entry.usdt} /></dl><CustodyBalanceErrors entry={entry} t={t} /></div>{index < entries.length - 1 ? <Separator /> : null}</div>)}</div><div className="hidden sm:block"><Table><TableHeader><TableRow><TableHead>{t("network")}</TableHead><TableHead>{t("nativeToken")}</TableHead><TableHead>USDC</TableHead><TableHead>USDT</TableHead></TableRow></TableHeader><TableBody>{entries.map((entry) => <TableRow key={entry.chain_id}><TableCell className="font-medium">{entry.network_name}</TableCell><TableCell><CustodyAssetAmount t={t} balance={entry.native} /></TableCell><TableCell><CustodyAssetAmount t={t} balance={entry.usdc} /></TableCell><TableCell><CustodyAssetAmount t={t} balance={entry.usdt} /></TableCell></TableRow>)}</TableBody></Table></div></>
}

function CustodyBalanceItem({ t, label, balance }: { t: Translate; label: string; balance: CustodyAssetBalance }) {
  return <div className="flex min-w-0 flex-col gap-1"><dt className="text-muted-foreground text-xs">{label}</dt><dd><CustodyAssetAmount t={t} balance={balance} /></dd></div>
}

function CustodyAssetAmount({ t, balance }: { t: Translate; balance: CustodyAssetBalance }) {
  return <span className="font-medium tabular-nums" title={balance.error ?? undefined}>{balance.amount ?? t("unavailable")}</span>
}

function CustodyBalanceErrors({ t, entry }: { t: Translate; entry: CustodyNetworkBalances }) {
  const errors = [entry.native, entry.usdc, entry.usdt].filter((asset) => asset.error)
  return errors.length ? <Alert variant="destructive"><AlertTitle>{t("requestFailed")}</AlertTitle><AlertDescription>{errors.map((asset) => `${asset.symbol}: ${asset.error}`).join(" · ")}</AlertDescription></Alert> : null
}

function RootSweepHistory({ t, language }: { t: Translate; language: Language }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const deposits = useQuery({ queryKey: ["admin-deposits"], queryFn: () => api<AdminDeposit[]>("/api/admin/deposits") })
  const [selected, setSelected] = useState<AdminDeposit | null>(null)
  const retry = useMutation({ mutationFn: (depositId: string) => api<Deposit>(`/api/admin/deposits/${depositId}/sweep`, { method: "POST" }), onSuccess: () => { toast.success(t("collectionRetryStarted")); setSelected(null) }, onError: (error) => showApiError(error, t), onSettled: () => void queryClient.invalidateQueries({ queryKey: ["admin-deposits"] }) })
  if (deposits.isLoading) return <Skeleton className="h-72 w-full" />
  if (deposits.error) return <RequestError t={t} error={deposits.error} />
  const rows = deposits.data ?? []
  return <><Card><CardHeader><CardTitle>{t("collectionOperations")}</CardTitle><CardDescription>{t("collectionOperationsBody")}</CardDescription><CardAction><Button aria-label={t("refresh")} size="icon" variant="ghost" disabled={deposits.isFetching} onClick={() => void deposits.refetch()}><RefreshCwIcon /></Button></CardAction></CardHeader><CardContent>{rows.length ? <SweepHistoryTable t={t} language={language} entries={rows} onRetry={setSelected} /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><LandmarkIcon /></EmptyMedia><EmptyTitle>{t("noCollectionOperations")}</EmptyTitle><EmptyDescription>{t("noCollectionOperationsBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card><RetrySweepDrawer t={t} deposit={selected} pending={retry.isPending} onOpenChange={(open) => !open && setSelected(null)} onConfirm={() => selected && retry.mutate(selected.id)} /></>
}

function RootConfigPage({ t }: { t: Translate }) {
  return <section className="flex flex-col gap-6"><AdminPageHeading t={t} title={t("custody")} body={t("rootConfigBody")} /><RootConfig t={t} /></section>
}

function RootDepositDiscoveryPage({ t }: { t: Translate }) {
  const api = useApi()
  const status = useQuery({ queryKey: ["deposit-discovery"], queryFn: () => api<DepositDiscoveryStatus>("/api/admin/deposit-discovery") })
  if (status.isLoading) return <Skeleton className="h-64 w-full" />
  if (status.error) return <RequestError t={t} error={status.error} />
  const data = status.data
  return <section className="flex flex-col gap-6"><AdminPageHeading t={t} title={t("depositDiscovery")} body={t("rpcDiscoveryBody")} /><Card><CardHeader><CardTitle>{t("rpcDiscovery")}</CardTitle><CardDescription>{t("discoveryPolling")}: {data?.polling_interval_seconds} {t("seconds")}</CardDescription></CardHeader><CardContent>{!data?.last_attempt_at ? <Empty><EmptyHeader><EmptyMedia variant="icon"><SearchIcon /></EmptyMedia><EmptyTitle>{t("discoveryNotStarted")}</EmptyTitle><EmptyDescription>{t("rpcDiscoveryBody")}</EmptyDescription></EmptyHeader></Empty> : <div className="flex flex-col gap-3"><div className="flex flex-wrap justify-between gap-2"><span className="text-muted-foreground">{t("discoveryLastAttempt")}</span><span className="font-medium">{formatTime(data.last_attempt_at)}</span></div>{data.last_success_at ? <div className="flex flex-wrap justify-between gap-2"><span className="text-muted-foreground">{t("discoveryLastSuccess")}</span><span className="font-medium">{formatTime(data.last_success_at)}</span></div> : null}{data.last_error ? <Alert variant="destructive"><AlertTitle>{t("discoveryLastError")}</AlertTitle><AlertDescription>{data.last_error}</AlertDescription></Alert> : null}</div>}</CardContent></Card></section>
}

function RootCollectionsPage({ t, language }: { t: Translate; language: Language }) {
  return <section className="flex flex-col gap-6"><AdminPageHeading t={t} title={t("collectionOperations")} body={t("collectionOperationsBody")} /><RootSweepHistory t={t} language={language} /></section>
}

function RootWithdrawalsPage({ t, language }: { t: Translate; language: Language }) {
  return <section className="flex flex-col gap-6"><AdminPageHeading t={t} title={t("withdrawalOperations")} body={t("withdrawalOperationsBody")} /><RootWithdrawalHistory t={t} language={language} /></section>
}

function RootWithdrawalHistory({ t, language }: { t: Translate; language: Language }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const withdrawals = useQuery({ queryKey: ["admin-withdrawals"], queryFn: () => api<AdminWithdrawal[]>("/api/admin/withdrawals") })
  const [selected, setSelected] = useState<AdminWithdrawal | null>(null)
  const retry = useMutation({ mutationFn: (withdrawalId: string) => api<Withdrawal>(`/api/admin/withdrawals/${withdrawalId}/retry`, { method: "POST" }), onSuccess: () => { toast.success(t("withdrawalRetryStarted")); setSelected(null) }, onError: (error) => showApiError(error, t), onSettled: () => void queryClient.invalidateQueries({ queryKey: ["admin-withdrawals"] }) })
  if (withdrawals.isLoading) return <Skeleton className="h-72 w-full" />
  if (withdrawals.error) return <RequestError t={t} error={withdrawals.error} />
  const rows = withdrawals.data ?? []
  return <><Card><CardHeader><CardTitle>{t("withdrawalOperations")}</CardTitle><CardDescription>{t("withdrawalOperationsBody")}</CardDescription><CardAction><Button aria-label={t("refresh")} size="icon" variant="ghost" disabled={withdrawals.isFetching} onClick={() => void withdrawals.refetch()}><RefreshCwIcon data-icon="inline-start" /></Button></CardAction></CardHeader><CardContent>{rows.length ? <WithdrawalHistoryTable t={t} language={language} entries={rows} onRetry={setSelected} /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><ArrowUpFromLineIcon /></EmptyMedia><EmptyTitle>{t("noWithdrawalOperations")}</EmptyTitle><EmptyDescription>{t("noWithdrawalOperationsBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card><RetryWithdrawalDrawer t={t} withdrawal={selected} pending={retry.isPending} onOpenChange={(open) => !open && setSelected(null)} onConfirm={() => selected && retry.mutate(selected.id)} /></>
}

function AdminBalancesPage({ t, language }: { t: Translate; language: Language }) {
  const api = useApi()
  const balances = useQuery({ queryKey: ["admin-balances"], queryFn: () => api<AdminBalances>("/api/admin/balances") })
  if (balances.isLoading) return <LoadingScreen t={t} />
  if (balances.error) return <RequestError t={t} error={balances.error} />
  const data = balances.data
  if (!data) return null
  return <section className="flex flex-col gap-6"><AdminPageHeading t={t} title={t("userBalancesTitle")} body={t("userBalancesBody")} /><Card><CardContent className="pt-6"><dl className="grid gap-5 sm:grid-cols-3"><div className="flex flex-col gap-1"><dt className="text-sm text-muted-foreground">{t("totalHeld")}</dt><dd className="text-2xl font-semibold tabular-nums">${formatUsd(data.summary.total_available_usd)}</dd></div><div className="flex flex-col gap-1"><dt className="text-sm text-muted-foreground">{t("fundedAccounts")}</dt><dd className="text-2xl font-semibold tabular-nums">{data.summary.funded_user_count}</dd></div><div className="flex flex-col gap-1"><dt className="text-sm text-muted-foreground">{t("allAccounts")}</dt><dd className="text-2xl font-semibold tabular-nums">{data.summary.user_count}</dd></div></dl></CardContent></Card><Card><CardHeader><CardTitle>{t("userBalances")}</CardTitle><CardDescription>{t("userBalancesBody")}</CardDescription></CardHeader><CardContent>{data.users.length ? <UserBalanceTable t={t} language={language} users={data.users} /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><UsersRoundIcon /></EmptyMedia><EmptyTitle>{t("noUsers")}</EmptyTitle><EmptyDescription>{t("userBalancesBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card></section>
}

function UserBalanceTable({ t, users }: { t: Translate; language?: Language; users: AdminUserBalance[] }) {
  return <><div className="flex flex-col sm:hidden">{users.map((user, index) => <div key={user.user_id}><div className="flex items-start justify-between gap-3 py-3"><div className="min-w-0"><span className="text-sm text-muted-foreground">{t("user")}</span><UserIdentity userId={user.user_id} /><span className="text-muted-foreground text-xs">{formatTime(user.created_at)}</span></div><span className="font-medium tabular-nums">${formatUsd(user.available_usd)}</span></div>{index < users.length - 1 ? <Separator /> : null}</div>)}</div><div className="hidden sm:block"><Table><TableHeader><TableRow><TableHead>{t("user")}</TableHead><TableHead>{t("time")}</TableHead><TableHead className="text-right">{t("balance")}</TableHead></TableRow></TableHeader><TableBody>{users.map((user) => <TableRow key={user.user_id}><TableCell><UserIdentity compact userId={user.user_id} /></TableCell><TableCell className="text-muted-foreground">{formatTime(user.created_at)}</TableCell><TableCell className="text-right font-medium tabular-nums">${formatUsd(user.available_usd)}</TableCell></TableRow>)}</TableBody></Table></div></>
}

function UserIdentity({ userId, compact = false }: { userId: string; language?: Language; compact?: boolean }) {
  return <span className="min-w-0"><LinkitUserInfo compact={compact} userId={userId} /></span>
}

function AdminLedgerPage({ t, language }: { t: Translate; language: Language }) {
  const api = useApi()
  const [kind, setKind] = useState("")
  const [status, setStatus] = useState("")
  const [draftUserId, setDraftUserId] = useState("")
  const [userId, setUserId] = useState("")
  const [offset, setOffset] = useState(0)
  const query = useMemo(() => { const parameters = new URLSearchParams({ limit: "50", offset: String(offset) }); if (kind) parameters.set("kind", kind); if (status) parameters.set("status", status); if (userId) parameters.set("user_id", userId); return parameters.toString() }, [kind, offset, status, userId])
  const ledger = useQuery({ queryKey: ["admin-ledger", query], queryFn: () => api<AdminLedgerPage>(`/api/admin/ledger?${query}`) })
  const data = ledger.data
  const applyFilters = (event: FormEvent) => { event.preventDefault(); setUserId(draftUserId.trim()); setOffset(0) }
  const updateKind = (value: string | null) => { setKind(value ?? ""); setOffset(0) }
  const updateStatus = (value: string | null) => { setStatus(value ?? ""); setOffset(0) }
  if (ledger.isLoading) return <LoadingScreen t={t} />
  if (ledger.error) return <RequestError t={t} error={ledger.error} />
  if (!data) return null
  const kindItems = [{ value: "all", label: t("allKinds") }, { value: "deposit", label: t("deposit") }, { value: "withdrawal", label: t("withdraw") }, { value: "transfer_in", label: t("transferIn") }, { value: "transfer_out", label: t("transferOut") }, { value: "adjustment", label: t("adjustment") }]
  const statusItems = [{ value: "all", label: t("allStatuses") }, { value: "pending", label: t("pending") }, { value: "posted", label: t("posted") }, { value: "rejected", label: t("rejected") }, { value: "disabled", label: t("disabled") }]
  const page = Math.floor(data.offset / data.limit) + 1
  return <section className="flex flex-col gap-6"><AdminPageHeading t={t} title={t("globalLedgerTitle")} body={t("globalLedgerBody")} /><Card><CardContent className="pt-6"><form onSubmit={applyFilters}><FieldGroup className="grid gap-3 sm:grid-cols-4"><Field><FieldLabel>{t("action")}</FieldLabel><Select items={kindItems} value={kind || "all"} onValueChange={(value) => updateKind(value === "all" ? null : value)}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{kindItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel>{t("status")}</FieldLabel><Select items={statusItems} value={status || "all"} onValueChange={(value) => updateStatus(value === "all" ? null : value)}><SelectTrigger className="w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{statusItems.map((item) => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><LinkitUserPicker name="user_id" value={draftUserId} onValueChange={(value) => setDraftUserId(value)} label={t("user")} lang={language === "zh" ? "zh-CN" : "en"} /></Field><Field className="self-end"><Button type="submit">{t("filter")}</Button></Field></FieldGroup></form></CardContent></Card><Card><CardHeader><CardTitle>{t("globalLedger")}</CardTitle><CardDescription>{t("totalEntries")}: {data.total}</CardDescription><CardAction><Button aria-label={t("refresh")} size="icon" variant="ghost" disabled={ledger.isFetching} onClick={() => void ledger.refetch()}><RefreshCwIcon /></Button></CardAction></CardHeader><CardContent>{data.entries.length ? <AdminLedgerTable t={t} language={language} entries={data.entries} /> : <Empty><EmptyHeader><EmptyMedia variant="icon"><ScrollTextIcon /></EmptyMedia><EmptyTitle>{t("noLedgerEntries")}</EmptyTitle><EmptyDescription>{t("globalLedgerBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent>{data.total > data.limit ? <CardFooter className="justify-between"><span className="text-sm text-muted-foreground">{t("page")} {page}</span><div className="flex items-center gap-2"><Button size="sm" variant="outline" disabled={data.offset === 0} onClick={() => setOffset(Math.max(0, data.offset - data.limit))}><ChevronLeftIcon data-icon="inline-start" />{t("previous")}</Button><Button size="sm" variant="outline" disabled={data.offset + data.limit >= data.total} onClick={() => setOffset(data.offset + data.limit)}>{t("next")}<ChevronRightIcon data-icon="inline-end" /></Button></div></CardFooter> : null}</Card></section>
}

function AdminLedgerTable({ t, language, entries }: { t: Translate; language: Language; entries: AdminLedgerEntry[] }) {
  return <><div className="flex flex-col sm:hidden">{entries.map((entry, index) => <div key={entry.id}><div className="flex items-start justify-between gap-3 py-3"><div className="min-w-0 flex-1"><span className="text-sm text-muted-foreground">{t("user")}</span><UserIdentity userId={entry.user_id} language={language} /><div className="mt-1 flex flex-wrap items-center gap-2"><span className="font-medium capitalize">{ledgerActionLabel(entry.kind, t)}</span><StatusBadge t={t} status={entry.status} /></div><ChainTransactionDetails entry={entry} t={t} /><span className="mt-1 text-muted-foreground text-xs">{formatTime(entry.created_at)}</span></div><span className="shrink-0 font-medium tabular-nums">{entry.balance_delta_usd_micros < 0 ? "−" : "+"}${formatMicros(Math.abs(entry.balance_delta_usd_micros))}</span></div>{index < entries.length - 1 ? <Separator /> : null}</div>)}</div><div className="hidden sm:block"><Table><TableHeader><TableRow><TableHead>{t("user")}</TableHead><TableHead>{t("action")}</TableHead><TableHead>{t("blockchain")}</TableHead><TableHead>{t("status")}</TableHead><TableHead className="text-right">{t("amount")}</TableHead><TableHead>{t("time")}</TableHead></TableRow></TableHeader><TableBody>{entries.map((entry) => <TableRow key={entry.id}><TableCell><UserIdentity userId={entry.user_id} language={language} compact /></TableCell><TableCell><div className="flex flex-col gap-1"><span className="font-medium capitalize">{ledgerActionLabel(entry.kind, t)}</span><span className="max-w-32 truncate text-muted-foreground text-xs">{entry.note ?? entry.external_reference ?? entry.id}</span></div></TableCell><TableCell><ChainTransactionDetails entry={entry} t={t} /></TableCell><TableCell><StatusBadge t={t} status={entry.status} /></TableCell><TableCell className="text-right font-medium tabular-nums">{entry.balance_delta_usd_micros < 0 ? "−" : "+"}${formatMicros(Math.abs(entry.balance_delta_usd_micros))}</TableCell><TableCell className="text-muted-foreground">{formatTime(entry.created_at)}</TableCell></TableRow>)}</TableBody></Table></div></>
}

function AdminPageHeading({ t, title, body }: { t: Translate; title: string; body: string }) {
  return <div><p className="text-sm text-muted-foreground">{t("administration")}</p><h1 className="text-2xl font-semibold tracking-tight">{title}</h1><p className="mt-1 max-w-3xl text-sm text-muted-foreground">{body}</p></div>
}

function SweepHistoryTable({ t, language, entries, onRetry }: { t: Translate; language: Language; entries: AdminDeposit[]; onRetry: (entry: AdminDeposit) => void }) {
  return <><div className="flex flex-col sm:hidden">{entries.map((entry, index) => <div key={entry.id}><SweepHistoryDetails t={t} language={language} entry={entry} /><div className="flex justify-end pb-3">{canRetrySweep(entry.sweep_status) ? <Button size="sm" variant="outline" onClick={() => onRetry(entry)}><RotateCcwIcon data-icon="inline-start" />{t("retryCollection")}</Button> : null}</div>{index < entries.length - 1 ? <Separator /> : null}</div>)}</div><div className="hidden sm:block"><Table><TableHeader><TableRow><TableHead>{t("details")}</TableHead><TableHead>{t("asset")}</TableHead><TableHead>{t("status")}</TableHead><TableHead className="text-right">{t("amount")}</TableHead><TableHead>{t("time")}</TableHead><TableHead className="text-right"><span className="sr-only">{t("retryCollection")}</span></TableHead></TableRow></TableHeader><TableBody>{entries.map((entry) => <TableRow key={entry.id}><TableCell><SweepReferences t={t} language={language} entry={entry} /></TableCell><TableCell><div className="flex flex-col gap-1"><span>{entry.asset_symbol}</span><span className="text-muted-foreground text-xs">{entry.network_name}</span></div></TableCell><TableCell><SweepStatusBadge t={t} status={entry.sweep_status} /></TableCell><TableCell className="text-right"><span className="font-medium tabular-nums">${entry.amount_usd}</span></TableCell><TableCell className="whitespace-nowrap text-muted-foreground">{formatTime(entry.created_at)}</TableCell><TableCell className="text-right">{canRetrySweep(entry.sweep_status) ? <Button size="sm" variant="outline" onClick={() => onRetry(entry)}><RotateCcwIcon data-icon="inline-start" />{t("retryCollection")}</Button> : null}</TableCell></TableRow>)}</TableBody></Table></div></>
}

function SweepHistoryDetails({ t, language, entry }: { t: Translate; language: Language; entry: AdminDeposit }) {
  return <div className="flex flex-col gap-2 py-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="font-medium tabular-nums">${entry.amount_usd}</span><Badge variant="outline">{entry.asset_symbol}</Badge><SweepStatusBadge t={t} status={entry.sweep_status} /></div><span className="text-muted-foreground text-xs">{entry.network_name} · {formatTime(entry.created_at)}</span></div></div><SweepReferences t={t} language={language} entry={entry} /></div>
}

function WithdrawalHistoryTable({ t, language, entries, onRetry }: { t: Translate; language: Language; entries: AdminWithdrawal[]; onRetry: (entry: AdminWithdrawal) => void }) {
  return <><div className="flex flex-col sm:hidden">{entries.map((entry, index) => <div key={entry.id}><WithdrawalHistoryDetails t={t} language={language} entry={entry} /><div className="flex justify-end pb-3">{entry.retryable ? <Button size="sm" variant="outline" onClick={() => onRetry(entry)}><RotateCcwIcon data-icon="inline-start" />{t("retryWithdrawal")}</Button> : null}</div>{index < entries.length - 1 ? <Separator /> : null}</div>)}</div><div className="hidden sm:block"><Table><TableHeader><TableRow><TableHead>{t("details")}</TableHead><TableHead>{t("asset")}</TableHead><TableHead>{t("status")}</TableHead><TableHead className="text-right">{t("amount")}</TableHead><TableHead>{t("time")}</TableHead><TableHead className="text-right"><span className="sr-only">{t("retryWithdrawal")}</span></TableHead></TableRow></TableHeader><TableBody>{entries.map((entry) => <TableRow key={entry.id}><TableCell><WithdrawalReferences t={t} language={language} entry={entry} /></TableCell><TableCell><div className="flex flex-col gap-1"><span>{entry.asset_symbol}</span><span className="text-muted-foreground text-xs">{entry.network_name}</span></div></TableCell><TableCell><StatusBadge t={t} status={entry.status} /></TableCell><TableCell className="text-right"><span className="font-medium tabular-nums">${entry.amount_usd}</span></TableCell><TableCell className="whitespace-nowrap text-muted-foreground">{formatTime(entry.created_at)}</TableCell><TableCell className="text-right">{entry.retryable ? <Button size="sm" variant="outline" onClick={() => onRetry(entry)}><RotateCcwIcon data-icon="inline-start" />{t("retryWithdrawal")}</Button> : null}</TableCell></TableRow>)}</TableBody></Table></div></>
}

function WithdrawalHistoryDetails({ t, language, entry }: { t: Translate; language: Language; entry: AdminWithdrawal }) {
  return <div className="flex flex-col gap-2 py-3"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="font-medium tabular-nums">${entry.amount_usd}</span><Badge variant="outline">{entry.asset_symbol}</Badge><StatusBadge t={t} status={entry.status} /></div><span className="text-muted-foreground text-xs">{entry.network_name} · {formatTime(entry.created_at)}</span></div></div><WithdrawalReferences t={t} language={language} entry={entry} /></div>
}

function WithdrawalReferences({ t, language, entry }: { t: Translate; language: Language; entry: AdminWithdrawal }) {
  return <div className="flex min-w-0 flex-col gap-1 text-xs"><span className="text-muted-foreground">{t("user")}</span><UserIdentity userId={entry.user_id} language={language} compact /><span className="text-muted-foreground">{t("withdrawalDestination")}</span><code className="truncate">{entry.destination_address}</code>{entry.transaction_hash ? <><span className="text-muted-foreground">{t("transactionHash")}</span><code className="truncate">{entry.transaction_hash}</code></> : null}{entry.last_error ? <Alert variant="destructive"><AlertTitle>{t("withdrawalError")}</AlertTitle><AlertDescription>{entry.last_error}</AlertDescription></Alert> : null}</div>
}

function RetryWithdrawalDrawer({ t, withdrawal, pending, onOpenChange, onConfirm }: { t: Translate; withdrawal: AdminWithdrawal | null; pending: boolean; onOpenChange: (open: boolean) => void; onConfirm: () => void }) {
  return <Drawer open={Boolean(withdrawal)} onOpenChange={onOpenChange}><DrawerContent><DrawerHeader><DrawerTitle>{t("retryWithdrawalTitle")}</DrawerTitle><DrawerDescription>{t("retryWithdrawalBody")}</DrawerDescription></DrawerHeader><div className="flex flex-col gap-3 px-4 py-5"><div className="flex flex-wrap items-center gap-2"><span className="font-medium tabular-nums">${withdrawal?.amount_usd}</span><Badge variant="outline">{withdrawal?.asset_symbol}</Badge>{withdrawal ? <StatusBadge t={t} status={withdrawal.status} /> : null}</div><Alert><AlertTitle>{t("retryWithdrawalNotice")}</AlertTitle><AlertDescription>{withdrawal?.transaction_hash ? t("transactionHash") : t("awaitingSigner")}</AlertDescription></Alert></div><DrawerFooter><Button disabled={pending} onClick={onConfirm}>{pending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("retryWithdrawal")}</Button><Button variant="outline" disabled={pending} onClick={() => onOpenChange(false)}>{t("cancel")}</Button></DrawerFooter></DrawerContent></Drawer>
}

function SweepReferences({ t, language, entry }: { t: Translate; language: Language; entry: AdminDeposit }) {
  return <div className="flex min-w-0 flex-col gap-1 text-xs"><span className="text-muted-foreground">{t("collectionUser")}</span><UserIdentity userId={entry.user_id} language={language} compact /><span className="text-muted-foreground">{t("depositAddress")}</span><code className="truncate">{entry.deposit_address}</code><span className="text-muted-foreground">{t("transactionHash")}</span><code className="truncate">{entry.transaction_hash}</code>{entry.gas_transaction_hash ? <><span className="text-muted-foreground">{t("collectionGasTransaction")}</span><code className="truncate">{entry.gas_transaction_hash}</code></> : null}{entry.token_transaction_hash ? <><span className="text-muted-foreground">{t("collectionTokenTransaction")}</span><code className="truncate">{entry.token_transaction_hash}</code></> : null}{entry.sweep_error_message ? <Alert variant="destructive"><AlertTitle>{t("collectionError")}</AlertTitle><AlertDescription>{entry.sweep_error_message}</AlertDescription></Alert> : null}</div>
}

function RetrySweepDrawer({ t, deposit, pending, onOpenChange, onConfirm }: { t: Translate; deposit: AdminDeposit | null; pending: boolean; onOpenChange: (open: boolean) => void; onConfirm: () => void }) {
  return <Drawer open={Boolean(deposit)} onOpenChange={onOpenChange}><DrawerContent><DrawerHeader><DrawerTitle>{t("retryCollectionTitle")}</DrawerTitle><DrawerDescription>{t("retryCollectionBody")}</DrawerDescription></DrawerHeader><div className="flex flex-col gap-3 px-4 py-5"><div className="flex flex-wrap items-center gap-2"><span className="font-medium tabular-nums">${deposit?.amount_usd}</span><Badge variant="outline">{deposit?.asset_symbol}</Badge>{deposit ? <SweepStatusBadge t={t} status={deposit.sweep_status} /> : null}</div><Alert><AlertTitle>{t("retryCollectionNotice")}</AlertTitle><AlertDescription>{deposit?.network_name}</AlertDescription></Alert></div><DrawerFooter><Button disabled={pending} onClick={onConfirm}>{pending && <LoaderCircleIcon data-icon="inline-start" className="animate-spin" />}{t("retryCollection")}</Button><Button variant="outline" disabled={pending} onClick={() => onOpenChange(false)}>{t("cancel")}</Button></DrawerFooter></DrawerContent></Drawer>
}

function AddressBookCard({ t }: { t: Translate }) {
  const api = useApi()
  const addressBook = useQuery({ queryKey: ["address-book"], queryFn: () => api<AddressBookEntry[]>("/api/address-book/me") })
  return <Card><CardHeader><CardTitle>{t("addressBook")}</CardTitle><CardDescription>{t("addressBookBody")}</CardDescription></CardHeader><CardContent>{addressBook.isLoading ? <Skeleton className="h-20 w-full" /> : addressBook.data?.length ? <div className="flex flex-col gap-3">{addressBook.data.map((entry) => <AddressBookRow key={entry.id} entry={entry} t={t} />)}</div> : <Empty><EmptyHeader><EmptyMedia variant="icon"><LandmarkIcon /></EmptyMedia><EmptyTitle>{t("addressBook")}</EmptyTitle><EmptyDescription>{t("addressBookBody")}</EmptyDescription></EmptyHeader></Empty>}</CardContent></Card>
}

function AddressBookRow({ entry, t }: { entry: AddressBookEntry; t: Translate }) {
  const api = useApi()
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState(false)
  const [label, setLabel] = useState(entry.label)
  const rename = useMutation({ mutationFn: () => api<AddressBookEntry>(`/api/address-book/me/${entry.id}`, { method: "PUT", body: { label } }), onSuccess: () => { setEditing(false); void queryClient.invalidateQueries({ queryKey: ["address-book"] }) }, onError: (error) => showApiError(error, t) })
  const remove = useMutation({ mutationFn: () => api<void>(`/api/address-book/me/${entry.id}`, { method: "DELETE" }), onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["address-book"] }), onError: (error) => showApiError(error, t) })
  return <div className="flex flex-wrap items-center justify-between gap-3"><div className="min-w-0"><div className="flex items-center gap-2">{editing ? <form onSubmit={(event) => { event.preventDefault(); rename.mutate() }}><FieldGroup className="flex-row items-center gap-2"><Field><FieldLabel className="sr-only" htmlFor={`address-book-label-${entry.id}`}>{t("addressLabel")}</FieldLabel><Input id={`address-book-label-${entry.id}`} value={label} onChange={(event) => setLabel(event.target.value)} maxLength={80} /></Field><Button size="sm" type="submit" disabled={rename.isPending || !label.trim()}>{t("saveAddress")}</Button><Button size="sm" type="button" variant="ghost" onClick={() => { setLabel(entry.label); setEditing(false) }}>{t("cancel")}</Button></FieldGroup></form> : <><span className="font-medium">{entry.label}</span><Badge variant="outline">{entry.chain_name}</Badge></>}</div><code className="block max-w-72 truncate text-xs text-muted-foreground">{entry.address}</code></div>{!editing ? <div className="flex items-center gap-1"><Button size="sm" variant="ghost" onClick={() => setEditing(true)}>{t("renameAddress")}</Button><Button size="sm" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate()}>{t("removeAddress")}</Button></div> : null}</div>
}

function NavigationDrawer({ currentPath, t, root, open, setOpen }: { currentPath: string; t: Translate; root: boolean; open: boolean; setOpen: (open: boolean) => void }) {
  const item = (path: string, label: string, icon: React.ReactNode) => <Button key={path} variant={currentPath === path ? "secondary" : "ghost"} className="w-full justify-start" render={<Link to={path} />} nativeButton={false} onClick={() => setOpen(false)}>{icon}{label}</Button>
  return <Drawer open={open} onOpenChange={setOpen} swipeDirection="left"><DrawerContent><DrawerHeader><DrawerTitle>{t("appName")}</DrawerTitle><DrawerDescription>{t("usdOnly")}</DrawerDescription></DrawerHeader><div className="flex min-h-0 flex-1 flex-col gap-6 overflow-y-auto p-4"><NavigationGroup label={t("account")}>{item("/", t("home"), <WalletCardsIcon data-icon="inline-start" />)}{item("/activity", t("activity"), <HistoryIcon data-icon="inline-start" />)}{item("/withdrawal-address-book", t("withdrawalAddressBook"), <ArrowUpFromLineIcon data-icon="inline-start" />)}{item("/automatic-receipts", t("automaticReceipts"), <ArrowDownToLineIcon data-icon="inline-start" />)}{item("/automatic-payments", t("automaticPayments"), <ArrowLeftRightIcon data-icon="inline-start" />)}{item("/settings", t("settings"), <SettingsIcon data-icon="inline-start" />)}</NavigationGroup>{root ? <NavigationGroup label={t("administration")}>{item("/admin/custody", t("custody"), <ShieldCheckIcon data-icon="inline-start" />)}{item("/admin/deposit-discovery", t("depositDiscovery"), <SearchIcon data-icon="inline-start" />)}{item("/admin/collections", t("collectionOperations"), <LandmarkIcon data-icon="inline-start" />)}{item("/admin/withdrawals", t("withdrawalOperations"), <ArrowUpFromLineIcon data-icon="inline-start" />)}{item("/admin/balances", t("userBalances"), <UsersRoundIcon data-icon="inline-start" />)}{item("/admin/ledger", t("globalLedger"), <ScrollTextIcon data-icon="inline-start" />)}</NavigationGroup> : null}</div></DrawerContent></Drawer>
}

function NavigationGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return <section className="flex flex-col gap-1"><h2 className="px-2 text-xs font-medium text-muted-foreground">{label}</h2><nav aria-label={label} className="flex flex-col gap-1">{children}</nav></section>
}

function AdminRoute({ root, t, children }: { root: boolean; t: Translate; children: React.ReactNode }) {
  if (root) return children
  return <section className="flex flex-col gap-6"><div><p className="text-sm text-muted-foreground">{t("administration")}</p><h1 className="text-2xl font-semibold tracking-tight">{t("rootOnly")}</h1></div><Alert><ShieldCheckIcon /><AlertTitle>{t("rootOnly")}</AlertTitle><AlertDescription>{t("security")}</AlertDescription></Alert></section>
}

function LanguageMenu({ language, setLanguage, t }: { language: Language; setLanguage: (language: Language) => void; t: Translate }) {
  return <DropdownMenu><DropdownMenuTrigger render={<Button aria-label={t("language")} size="icon" variant="ghost" />}><Globe2Icon /></DropdownMenuTrigger><DropdownMenuContent align="end"><DropdownMenuGroup><DropdownMenuItem onClick={() => setLanguage("en")}><Globe2Icon />{t("english")}{language === "en" ? <Badge variant="secondary">EN</Badge> : null}</DropdownMenuItem><DropdownMenuItem onClick={() => setLanguage("zh")}><Globe2Icon />{t("chinese")}{language === "zh" ? <Badge variant="secondary">中文</Badge> : null}</DropdownMenuItem></DropdownMenuGroup></DropdownMenuContent></DropdownMenu>
}

function SweepStatusBadge({ t, status }: { t: Translate; status: string }) { const label = status === "queued" ? t("collectionQueued") : status === "awaiting_configuration" ? t("collectionAwaitingConfiguration") : status === "submitted" ? t("collectionSubmitted") : status === "failed" ? t("collectionFailed") : status === "swept" ? t("collectionSwept") : status.replace("_", " "); return <Badge variant={status === "failed" ? "destructive" : status === "swept" ? "secondary" : "outline"}>{label}</Badge> }
function canRetrySweep(status: string) { return status === "queued" || status === "awaiting_configuration" || status === "failed" }
function formatTime(value: string) { return new Date(value).toLocaleString() }
function shortTransactionHash(value: string) { return `${value.slice(0, 10)}…${value.slice(-8)}` }
function shortAddress(value: string) { return `${value.slice(0, 8)}…${value.slice(-6)}` }
function agreementAuthorizationUrl(agreementId: string) { return `${window.location.origin}/#/sign_agreement?agreement_id=${agreementId}` }
function transactionExplorerUrl(chainId: number | null, transactionHash: string | null) { if (!chainId || !transactionHash) return null; const explorers: Record<number, string> = { 1: "https://etherscan.io", 10: "https://optimistic.etherscan.io", 56: "https://bscscan.com", 137: "https://polygonscan.com", 8453: "https://basescan.org", 42161: "https://arbiscan.io" }; const explorer = explorers[chainId]; return explorer ? `${explorer}/tx/${transactionHash}` : null }
function ledgerActionLabel(kind: string, t: Translate) { return kind === "deposit" ? t("deposit") : kind === "withdrawal" ? t("withdraw") : kind === "transfer_in" ? t("transferIn") : kind === "transfer_out" ? t("transferOut") : kind === "adjustment" ? t("adjustment") : kind.replace("_", " ") }
function StatusBadge({ t, status }: { t: Translate; status: string }) { const label = status === "pending" ? t("pending") : status === "posted" ? t("posted") : status === "rejected" ? t("rejected") : status === "disabled" ? t("disabled") : status === "awaiting_signer" ? t("awaitingSigner") : status === "submitted" ? t("submitted") : status === "completed" ? t("completed") : status === "failed" ? t("failed") : status; return <Badge variant={status === "failed" || status === "rejected" ? "destructive" : status === "completed" || status === "posted" ? "secondary" : "outline"}>{label}</Badge> }
function LoadingScreen({ t }: { t: Translate }) { return <div className="flex flex-col gap-4"><Skeleton className="h-8 w-40" /><Skeleton className="h-36 w-full" /><p className="text-muted-foreground">{t("loading")}</p></div> }
function RequestError({ t, error }: { t: Translate; error?: unknown }) { return <Alert variant="destructive"><AlertTitle>{t("requestFailed")}</AlertTitle><AlertDescription>{apiErrorDescription(error, t)}</AlertDescription></Alert> }

type Translate = (key: keyof typeof messages.en) => string
function useApi() { const auth = useAuthMini(); return useCallback(async <T,>(path: string, init: { method?: string; body?: unknown; idempotency?: string } = {}) => { const send = async (refresh: boolean) => { const snapshot = refresh ? await auth.sdk?.session.refresh() : auth.sdk?.session.getState(); const token = snapshot?.accessToken; if (!token) throw new ApiError(401, "Authentication is required"); const headers = new Headers({ Authorization: `Bearer ${token}` }); if (init.body !== undefined) headers.set("Content-Type", "application/json"); if (init.idempotency) headers.set("Idempotency-Key", init.idempotency); return fetch(path, { method: init.method ?? "GET", headers, body: init.body === undefined ? undefined : JSON.stringify(init.body) }) }; let response = await send(false); if (response.status === 401) response = await send(true); if (!response.ok) { const body = await response.json().catch(() => ({ error: response.statusText })) as { error?: string }; throw new ApiError(response.status, body.error ?? response.statusText) } return response.status === 204 ? undefined as T : response.json() as Promise<T> }, [auth.sdk]) }
async function publicRequest<T>(path: string) { const response = await fetch(path); if (!response.ok) throw new ApiError(response.status, response.statusText); return response.json() as Promise<T> }
function showApiError(error: unknown, t: Translate) { toast.error(t("requestFailed"), { description: apiErrorDescription(error, t) }) }
function apiErrorDescription(error: unknown, t: Translate) { if (!(error instanceof ApiError)) return t("unknownRequestError"); if (error.message.startsWith("EVM RPC operation failed")) return `${t("evmRpcUnavailable")} ${error.message}`; if (error.message.includes("not confirmed yet")) return t("transactionNotConfirmed"); if (error.message.includes("transaction reverted")) return t("transactionReverted"); if (error.message.includes("does not contain a configured") || error.message.includes("does not contain a supported")) return t("depositTransferMissing"); if (error.message.includes("already been credited")) return t("depositAlreadyCredited"); return error.message || t("unknownRequestError") }
function enabledNetworks(assets: Asset[]): Network[] { const networks = new Map<number, string>(); for (const asset of assets.filter((asset) => asset.enabled)) networks.set(asset.chain_id, asset.network_name ?? `Chain ${asset.chain_id}`); return [...networks].map(([chain_id, name]) => ({ chain_id, name })) }
function formatUsd(value?: string) { return value ? Number(value).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 }) : "0.00" }
function formatMicros(value: number) { return (value / 1_000_000).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 6 }) }
function usdToMicros(value: string) { if (!/^\d+(?:\.\d{1,6})?$/.test(value)) return 0; const [whole, fraction = ""] = value.split("."); const micros = BigInt(whole) * 1_000_000n + BigInt((fraction + "000000").slice(0, 6)); return micros <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(micros) : 0 }
function subjectFromToken(token?: string) { try { const value = token?.split(".")[1]; return value ? JSON.parse(atob(value.replace(/-/g, "+").replace(/_/g, "/"))).sub as string : null } catch { return null } }

createRoot(document.getElementById("root")!).render(<AppRoot />)
