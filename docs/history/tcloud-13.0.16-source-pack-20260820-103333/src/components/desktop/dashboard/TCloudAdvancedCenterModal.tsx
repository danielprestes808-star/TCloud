import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { load } from '@tauri-apps/plugin-store';
import {
  ArchiveRestore,
  Search,
  Database,
  Image as ImageIcon,
  RefreshCw,
  Trash2,
  Info,
  Copy,
  MoveRight,
  WandSparkles,
  CloudDownload,
  History,
  Share2,
  FolderSync,
  HardDrive,
  X,
  Plus,
  Folder,
  CheckCircle2,
  ShieldCheck,
  Play,
  Pin,
  Eraser,
  Gauge,
  Images,
  FolderSearch2,
  CopyCheck,
  Sparkles,
  Activity,
  HeartPulse,
  Bell,
  MonitorUp,
  Rocket,
  SlidersHorizontal,
  BatteryCharging,
  Bug,
  Tag,
  BarChart3,
  FileDown,
  Wrench,
  LockKeyhole,
  Music2,
  ScanText,
} from 'lucide-react';
import { toast } from 'sonner';
import { check } from '@tauri-apps/plugin-updater';
import { TCloudGallery } from './TCloudGallery';
import { clearClientErrors, listClientErrors, type TCloudClientError } from '../../../services/tcloudDiagnostics';
import { applyPerformanceProfile, getAutoEconomyEnabled, getPerformanceProfile, setAutoEconomyEnabled, type TCloudPerformanceProfile } from '../../../services/tcloudPerformance';
import type { TelegramFile, TelegramFolder, ShareInfo } from '../../../types';
import {
  addVersion,
  copyTelegramFiles,
  deleteBackup,
  deleteRule,
  formatBytesCompact,
  getProIndexStats,
  listBackups,
  listRules,
  listTrash,
  listVersions,
  markProIndexRebuilt,
  pinOffline,
  restoreTrash,
  saveBackup,
  saveRule,
  scanBackup,
  type ProBackupSource,
  type ProIndexStats,
  type ProRule,
  type ProTrashItem,
  type ProVersionItem,
  listDuplicates,
  getCleanupStats,
  searchIndexedContent,
  listActivity,
  addActivity,
  pauseAllJobs,
  sendDesktopNotification,
  type ProDuplicateGroup,
  type ProCleanupStats,
  type ProContentHit,
  type ProActivityItem,
  getFileMetadata,
  setFileMetadata,
  getStorageBreakdown,
  exportDiagnostics,
  exportConfigJson,
  importConfigJson,
  type ProFileMetadata,
  type ProStorageBucket,
} from '../../../services/tcloudProductivity';
import {
  reindexFullContent,
  queueIndexableDownloads,
  getCfapiStatus,
  setCfapiEnabled,
  populateCfapiPlaceholders,
  finalizeCfapiExplorer,
  checkCfapiIntegrity,
  keepCfapiOnDevice,
  freeCfapiSpace,
  openCfapiRoot,
  getWindowsHelloStatus,
  enrollWindowsHello,
  removeWindowsHello,
  unlockWithWindowsHello,
  getMusicLibrary,
  getMusicPlaylists,
  saveMusicPlaylist,
  deleteMusicPlaylist,
  applySmartCleanup,
  runDeepDiagnostics,
  getShareAudit,
  getRollbackStatus,
  forceRollback,
  getFolderCryptoPolicy,
  setFolderCryptoPolicy,
  type ContentReindexResult,
  type CfapiStatus,
  type WindowsHelloStatus,
  type MusicTrack,
  type MusicPlaylist,
  type DiagnosticCheck,
  type ShareAuditItem,
  type RollbackStatus,
  type FolderCryptoPolicy,
} from '../../../services/tcloudCompletion';
import {
  listFolderSyncPolicies,
  setFolderSyncPolicy,
  getSyncMaturitySettings,
  setSyncMaturitySettings,
  purgeTrash,
  listSyncConflicts,
  resolveSyncConflict,
  type FolderSyncPolicy,
  type FolderPolicyMode,
  type SyncMaturitySettings,
  type SyncConflict,
} from '../../../services/tcloudSyncMaturity';

export type TCloudAdvancedTab =
  | 'index'
  | 'cache'
  | 'sync'
  | 'policies'
  | 'conflicts'
  | 'trash'
  | 'search'
  | 'details'
  | 'move'
  | 'rules'
  | 'offline'
  | 'versions'
  | 'share'
  | 'backup'
  | 'drive'
  | 'gallery'
  | 'content'
  | 'smart'
  | 'duplicates'
  | 'cleanup'
  | 'activity'
  | 'health'
  | 'notifications'
  | 'tray'
  | 'updates'
  | 'onboarding'
  | 'profiles'
  | 'power'
  | 'errors'
  | 'metadata'
  | 'storage'
  | 'config'
  | 'developer'
  | 'security'
  | 'music';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  files: TelegramFile[];
  folders: TelegramFolder[];
  activeFolderId: number | null;
  selectedIds: number[];
  onSelectionChange: (ids: number[]) => void;
  onRefresh: () => void;
  onOpenSyncCenter: () => void;
  onOpenShare: (file: TelegramFile) => void;
  onSearch: (query: string) => void;
  onRunBackup: (backup: ProBackupSource) => Promise<void>;
  initialTab?: TCloudAdvancedTab;
}

const tabs: Array<{ id: TCloudAdvancedTab; label: string; icon: typeof Database; number: number }> = [
  { id: 'index', label: 'Índice local', icon: Database, number: 1 },
  { id: 'cache', label: 'Miniaturas e cache', icon: ImageIcon, number: 2 },
  { id: 'sync', label: 'Central de Sync', icon: RefreshCw, number: 3 },
  { id: 'policies', label: 'Disponibilidade por pasta', icon: Folder, number: 34 },
  { id: 'conflicts', label: 'Problemas de sincronização', icon: ShieldCheck, number: 35 },
  { id: 'trash', label: 'Lixeira', icon: Trash2, number: 4 },
  { id: 'search', label: 'Busca avançada', icon: Search, number: 5 },
  { id: 'details', label: 'Detalhes', icon: Info, number: 6 },
  { id: 'move', label: 'Mover / Copiar', icon: MoveRight, number: 7 },
  { id: 'rules', label: 'Organização automática', icon: WandSparkles, number: 8 },
  { id: 'offline', label: 'Disponível offline', icon: CloudDownload, number: 9 },
  { id: 'versions', label: 'Versões anteriores', icon: History, number: 10 },
  { id: 'share', label: 'Compartilhamento', icon: Share2, number: 11 },
  { id: 'backup', label: 'Backup automático', icon: FolderSync, number: 12 },
  { id: 'drive', label: 'Explorer', icon: HardDrive, number: 13 },
  { id: 'gallery', label: 'Galeria', icon: Images, number: 14 },
  { id: 'content', label: 'Busca por conteúdo', icon: FolderSearch2, number: 15 },
  { id: 'smart', label: 'Pastas inteligentes', icon: Sparkles, number: 16 },
  { id: 'duplicates', label: 'Duplicados', icon: CopyCheck, number: 17 },
  { id: 'cleanup', label: 'Limpeza inteligente', icon: Eraser, number: 18 },
  { id: 'activity', label: 'Atividades', icon: Activity, number: 19 },
  { id: 'health', label: 'Verificar problemas', icon: HeartPulse, number: 20 },
  { id: 'notifications', label: 'Notificações', icon: Bell, number: 21 },
  { id: 'tray', label: 'Bandeja', icon: MonitorUp, number: 22 },
  { id: 'updates', label: 'Atualizações', icon: Rocket, number: 23 },
  { id: 'onboarding', label: 'Primeira configuração', icon: CheckCircle2, number: 24 },
  { id: 'profiles', label: 'Perfis', icon: SlidersHorizontal, number: 25 },
  { id: 'power', label: 'Bateria e rede', icon: BatteryCharging, number: 26 },
  { id: 'errors', label: 'Erros e diagnóstico', icon: Bug, number: 27 },
  { id: 'metadata', label: 'Tags e notas', icon: Tag, number: 28 },
  { id: 'storage', label: 'Uso de espaço', icon: BarChart3, number: 29 },
  { id: 'config', label: 'Backup das configurações', icon: FileDown, number: 30 },
  { id: 'developer', label: 'Modo desenvolvimento', icon: Wrench, number: 31 },
  { id: 'security', label: 'Cofre e criptografia', icon: LockKeyhole, number: 32 },
  { id: 'music', label: 'Biblioteca de música', icon: Music2, number: 33 },
];

const SIMPLE_ADVANCED_TABS = new Set<TCloudAdvancedTab>(['offline','versions','trash','policies','conflicts','share','backup','gallery','music','storage','health']);

const selectedFiles = (files: TelegramFile[], ids: number[]) => files.filter((file) => ids.includes(file.id) && file.type !== 'folder');

export function TCloudAdvancedCenterModal({
  isOpen,
  onClose,
  files,
  folders,
  activeFolderId,
  selectedIds,
  onSelectionChange,
  onRefresh,
  onOpenSyncCenter,
  onOpenShare,
  onSearch,
  onRunBackup,
  initialTab = 'index',
}: Props) {
  const [showTechnicalTabs, setShowTechnicalTabs] = useState(() => localStorage.getItem('tcloud:advanced-ui') === 'true');
  const initialVisibleTab = showTechnicalTabs || SIMPLE_ADVANCED_TABS.has(initialTab) ? initialTab : 'health';
  const [tab, setTab] = useState<TCloudAdvancedTab>(initialVisibleTab);
  const [stats, setStats] = useState<ProIndexStats | null>(null);
  const [trash, setTrash] = useState<ProTrashItem[]>([]);
  const [versions, setVersions] = useState<ProVersionItem[]>([]);
  const [folderPolicies, setFolderPolicies] = useState<FolderSyncPolicy[]>([]);
  const [policyPath, setPolicyPath] = useState('Mensagens Salvas');
  const [policyMode, setPolicyMode] = useState<FolderPolicyMode>('normal');
  const [maturitySettings, setMaturitySettings] = useState<SyncMaturitySettings>({ version_retention: 10, trash_retention_days: 30, conflict_strategy: 'keep_both' });
  const [syncConflicts, setSyncConflicts] = useState<SyncConflict[]>([]);
  const [rules, setRules] = useState<ProRule[]>([]);
  const [backups, setBackups] = useState<ProBackupSource[]>([]);
  const [busy, setBusy] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [targetFolderId, setTargetFolderId] = useState<number | null>(null);
  const [ruleName, setRuleName] = useState('Imagens');
  const [ruleExtensions, setRuleExtensions] = useState('jpg,jpeg,png,webp');
  const [ruleTarget, setRuleTarget] = useState<number | null>(null);
  const [backupInterval, setBackupInterval] = useState(1440);
  const [trashRetentionDays, setTrashRetentionDays] = useState(() => Number(localStorage.getItem('tcloud:trash-retention-days') || '30'));
  const [duplicates, setDuplicates] = useState<ProDuplicateGroup[]>([]);
  const [cleanupStats, setCleanupStats] = useState<ProCleanupStats | null>(null);
  const [contentQuery, setContentQuery] = useState('');
  const [contentHits, setContentHits] = useState<ProContentHit[]>([]);
  const [activities, setActivities] = useState<ProActivityItem[]>([]);
  const [clientErrors, setClientErrors] = useState<TCloudClientError[]>(() => listClientErrors());
  const [profile, setProfile] = useState<TCloudPerformanceProfile>(() => getPerformanceProfile());
  const [autoEconomy, setAutoEconomy] = useState(() => getAutoEconomyEnabled());
  const [syncPaused, setSyncPaused] = useState(() => localStorage.getItem('tcloud:sync-paused') === 'true');
  const [autoCompressBatches, setAutoCompressBatches] = useState(() => localStorage.getItem('tcloud:auto-compress-batches') === 'true');
  const [autoCleanup, setAutoCleanup] = useState(() => localStorage.getItem('tcloud:auto-cleanup') === 'true');
  const [fileMetadata, setFileMetadataState] = useState<ProFileMetadata | null>(null);
  const [metadataTags, setMetadataTags] = useState('');
  const [metadataNote, setMetadataNote] = useState('');
  const [storageBuckets, setStorageBuckets] = useState<ProStorageBucket[]>([]);
  const [duplicateCheck, setDuplicateCheck] = useState(() => localStorage.getItem('tcloud:duplicate-check-before-upload') === 'true');
  const [ruleNameContains, setRuleNameContains] = useState('');
  const [ruleMinMb, setRuleMinMb] = useState('');
  const [ruleMaxMb, setRuleMaxMb] = useState('');
  const [debugOverlayEnabled, setDebugOverlayEnabled] = useState(() => localStorage.getItem('debug_overlay') === '1');
  const [shareHistory, setShareHistory] = useState<ShareInfo[]>([]);
  const [contentIndexResult, setContentIndexResult] = useState<ContentReindexResult | null>(null);
  const [cfapi, setCfapi] = useState<CfapiStatus | null>(null);
  const [hello, setHello] = useState<WindowsHelloStatus | null>(null);
  const [musicTracks, setMusicTracks] = useState<MusicTrack[]>([]);
  const [musicPlaylists, setMusicPlaylists] = useState<MusicPlaylist[]>([]);
  const [playlistName, setPlaylistName] = useState('Minha playlist');
  const [diagnostics, setDiagnostics] = useState<DiagnosticCheck[]>([]);
  const [shareAudit, setShareAudit] = useState<ShareAuditItem[]>([]);
  const [rollback, setRollback] = useState<RollbackStatus | null>(null);
  const [folderCrypto, setFolderCrypto] = useState<FolderCryptoPolicy | null>(null);
  const [vaultStatus, setVaultStatus] = useState<{exists:boolean;is_unlocked:boolean;has_recovery:boolean}|null>(null);
  const [vaultPassphrase, setVaultPassphrase] = useState('');

  const selection = useMemo(() => selectedFiles(files, selectedIds), [files, selectedIds]);
  const primaryFile = selection[0] ?? null;

  const refresh = useCallback(async () => {
    if (!isOpen) return;
    const [statsResult, trashResult, versionsResult, rulesResult, backupsResult, duplicateResult, cleanupResult, activityResult, storageResult, shareHistoryResult, policiesResult, maturityResult, conflictsResult] = await Promise.all([
      getProIndexStats().catch(() => null),
      listTrash().catch(() => []),
      listVersions().catch(() => []),
      listRules().catch(() => []),
      listBackups().catch(() => []),
      listDuplicates().catch(() => []),
      getCleanupStats().catch(() => null),
      listActivity(100).catch(() => []),
      getStorageBreakdown().catch(() => []),
      invoke<ShareInfo[]>('cmd_list_share_history').catch(() => []),
      listFolderSyncPolicies().catch(() => []),
      getSyncMaturitySettings().catch(() => ({ version_retention:10, trash_retention_days:30, conflict_strategy:'ask' as const })),
      listSyncConflicts().catch(() => []),
    ]);
    setStats(statsResult);
    setTrash(trashResult);
    setVersions(versionsResult);
    setRules(rulesResult);
    setBackups(backupsResult);
    setDuplicates(duplicateResult);
    setCleanupStats(cleanupResult);
    setActivities(activityResult);
    setStorageBuckets(storageResult);
    setShareHistory(shareHistoryResult);
    setFolderPolicies(policiesResult);
    setMaturitySettings(maturityResult);
    setTrashRetentionDays(maturityResult.trash_retention_days);
    setSyncConflicts(conflictsResult);
    const [cfapiResult, helloResult, tracksResult, playlistsResult, auditResult, rollbackResult, vaultResult] = await Promise.all([
      getCfapiStatus().catch(() => null),
      getWindowsHelloStatus().catch(() => null),
      getMusicLibrary().catch(() => []),
      getMusicPlaylists().catch(() => []),
      getShareAudit(null, 100).catch(() => []),
      getRollbackStatus().catch(() => null),
      invoke<{exists:boolean;is_unlocked:boolean;has_recovery:boolean}>('cmd_get_vault_status').catch(() => null),
    ]);
    setCfapi(cfapiResult); setHello(helloResult); setMusicTracks(tracksResult); setMusicPlaylists(playlistsResult);
    setShareAudit(auditResult); setRollback(rollbackResult); setVaultStatus(vaultResult);
  }, [isOpen]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!isOpen) return;
    setTab(showTechnicalTabs || SIMPLE_ADVANCED_TABS.has(initialTab) ? initialTab : 'health');
  }, [isOpen, initialTab, showTechnicalTabs]);

  useEffect(() => {
    const updateMode = () => setShowTechnicalTabs(localStorage.getItem('tcloud:advanced-ui') === 'true');
    window.addEventListener('tcloud-advanced-ui-changed', updateMode);
    return () => window.removeEventListener('tcloud-advanced-ui-changed', updateMode);
  }, []);

  useEffect(() => {
    const update = () => setClientErrors(listClientErrors());
    window.addEventListener('tcloud-errors-changed', update);
    return () => window.removeEventListener('tcloud-errors-changed', update);
  }, []);

  useEffect(() => {
    if (!isOpen || !primaryFile) {
      setFileMetadataState(null);
      setMetadataTags('');
      setMetadataNote('');
      return;
    }
    let alive = true;
    void getFileMetadata(primaryFile.id, primaryFile.folder_id ?? activeFolderId).then((value) => {
      if (!alive) return;
      setFileMetadataState(value);
      setMetadataTags(value.tags.join(', '));
      setMetadataNote(value.note);
    }).catch(() => {});
    return () => { alive = false; };
  }, [isOpen, primaryFile?.id, primaryFile?.folder_id, activeFolderId]);

  useEffect(() => {
    if (!isOpen || activeFolderId === null) { setFolderCrypto(null); return; }
    void getFolderCryptoPolicy(activeFolderId).then(setFolderCrypto).catch(() => setFolderCrypto(null));
  }, [isOpen, activeFolderId]);

  if (!isOpen) return null;

  const restoreItem = async (item: ProTrashItem) => {
    await restoreTrash(item.id);
    window.dispatchEvent(new Event('tcloud-trash-changed'));
    await refresh();
    await addActivity('trash', 'Arquivo restaurado', item.name).catch(() => null);
    toast.success(`${item.name} restaurado.`);
  };

  const purgeItem = async (item: ProTrashItem) => {
    if (!window.confirm(`Excluir definitivamente “${item.name}” do Telegram?`)) return;
    setBusy(true);
    try {
      await purgeTrash(item.id);
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      await refresh();
      onRefresh();
      await addActivity('trash', 'Arquivo excluído definitivamente', item.name).catch(() => null);
      toast.success('Exclusão definitiva adicionada à fila.');
    } catch (error) {
      toast.error(`Falha ao excluir: ${error}`);
    } finally {
      setBusy(false);
    }
  };

  const moveOrCopy = async (mode: 'move' | 'copy') => {
    if (!selection.length) return toast.info('Selecione pelo menos um arquivo.');
    if (targetFolderId === activeFolderId) return toast.info('Escolha outro destino.');
    setBusy(true);
    try {
      const groups = new Map<number | null, number[]>();
      for (const item of selection) {
        const source = item.folder_id ?? activeFolderId;
        groups.set(source, [...(groups.get(source) || []), item.id]);
      }
      for (const [sourceFolderId, messageIds] of groups) {
        if (mode === 'copy') {
          await copyTelegramFiles(messageIds, sourceFolderId, targetFolderId);
        } else {
          await invoke('cmd_move_files', { messageIds, sourceFolderId, targetFolderId });
        }
      }
      if (mode === 'move') onSelectionChange([]);
      await addActivity('files', mode === 'copy' ? 'Arquivos copiados' : 'Arquivos movidos', `${selection.length} arquivo(s)`).catch(() => null);
      toast.success(`${selection.length} arquivo(s) ${mode === 'copy' ? 'copiado(s)' : 'movido(s)'}.`);
      onRefresh();
    } catch (error) {
      toast.error(`${mode === 'copy' ? 'Cópia' : 'Movimentação'} falhou: ${error}`);
    } finally {
      setBusy(false);
    }
  };

  const createRule = async () => {
    if (!ruleName.trim()) return;
    await saveRule({
      name: ruleName.trim(),
      extensions: ruleExtensions.trim(),
      target_folder_id: ruleTarget,
      enabled: true,
      name_contains: ruleNameContains.trim() || null,
      min_size: ruleMinMb ? Math.max(0, Number(ruleMinMb)) * 1024 * 1024 : null,
      max_size: ruleMaxMb ? Math.max(0, Number(ruleMaxMb)) * 1024 * 1024 : null,
    });
    await refresh();
    await addActivity('rules', 'Regra automática criada', ruleName.trim()).catch(() => null);
    toast.success('Regra criada.');
  };

  const addBackupSource = async () => {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({ directory: true, multiple: false, title: 'Selecionar pasta para backup' });
    if (!picked || Array.isArray(picked)) return;
    await saveBackup({ local_path: picked, target_folder_id: targetFolderId, enabled: true, interval_minutes: backupInterval });
    await refresh();
    await addActivity('backup', 'Pasta adicionada ao backup', picked).catch(() => null);
    toast.success('Pasta adicionada ao backup automático.');
  };

  const content = (() => {
    if (tab === 'index') {
      return (
        <Feature title="Índice local instantâneo" description="O TCloud mantém metadados em SQLite para abrir a biblioteca sem depender de uma nova varredura completa do Telegram.">
          <MetricGrid values={[
            ['Arquivos indexados', stats?.files ?? 0],
            ['Pastas', stats?.folders ?? 0],
            ['Dados indexados', formatBytesCompact(stats?.indexed_bytes ?? 0)],
            ['Cache local', stats?.cached_files ?? 0],
            ['Jobs pendentes', stats?.pending_jobs ?? 0],
            ['Conflitos', stats?.conflicts ?? 0],
          ]} />
          <Action onClick={async () => { await markProIndexRebuilt(); await refresh(); onRefresh(); toast.success('Índice marcado para atualização e biblioteca recarregada.'); }} icon={RefreshCw}>Reindexar agora</Action>
        </Feature>
      );
    }
    if (tab === 'cache') {
      return (
        <Feature title="Miniaturas e cache persistente" description="Controle o cache de previews e reduza downloads repetidos ao navegar pelos mesmos arquivos.">
          <MetricGrid values={[[ 'Arquivos em cache', stats?.cached_files ?? 0 ], [ 'Disponíveis offline', stats?.offline_files ?? 0 ]]} />
          <div className="tcloud-pro-actions">
            <Action onClick={async () => { await invoke('cmd_clean_preview_cache'); toast.success('Cache de previews limpo.'); }} icon={Eraser}>Limpar previews</Action>
            <Action onClick={async () => { await invoke('cmd_clean_cache'); toast.success('Cache temporário limpo.'); }} icon={ArchiveRestore}>Limpar cache temporário</Action>
          </div>
          <Hint>As miniaturas visíveis continuam sendo priorizadas; arquivos fora da tela não disputam banda com uploads e sincronização.</Hint>
        </Feature>
      );
    }
    if (tab === 'sync') {
      return (
        <Feature title="Central de sincronização" description="Fila de upload/download, velocidade, progresso, conflitos e controles de prioridade.">
          <MetricGrid values={[[ 'Jobs ativos/pendentes', stats?.pending_jobs ?? 0 ], [ 'Conflitos aguardando', stats?.conflicts ?? 0 ]]} />
          <Action onClick={() => { onClose(); onOpenSyncCenter(); }} icon={Gauge}>Abrir Central de Sync</Action>
        </Feature>
      );
    }
    if (tab === 'policies') {
      return (
        <Feature title="Políticas por pasta" description="Defina como cada pasta do Explorer usa o disco. A política é persistente e também vale para arquivos que chegarem depois pelo Telegram.">
          <div className="tcloud-pro-actions">
            <input className="tcloud-pro-input" value={policyPath} onChange={(event)=>setPolicyPath(event.target.value)} placeholder="Ex.: Mensagens Salvas ou Projetos/2026" />
            <select className="tcloud-pro-input" value={policyMode} onChange={(event)=>setPolicyMode(event.target.value as FolderPolicyMode)}>
              <option value="normal">Normal / sob demanda</option>
              <option value="offline">Sempre manter offline</option>
              <option value="online">Somente online</option>
            </select>
            <Action icon={Folder} onClick={async()=>{try{await setFolderSyncPolicy(policyPath,policyMode);await refresh();toast.success('Política aplicada à pasta.');}catch(error){toast.error(`Falha ao aplicar política: ${error}`);}}}>Aplicar política</Action>
          </div>
          <Hint>Use caminho relativo à raiz TCloud. “Normal” remove a política explícita; “Sempre offline” fixa e hidrata; “Somente online” desafixa e libera espaço.</Hint>
          <div className="tcloud-pro-list">{folderPolicies.length===0?<Empty text="Nenhuma política específica. Todas as pastas usam o modo Normal."/>:folderPolicies.map((policy)=><div className="tcloud-pro-list-row" key={policy.relative_path}><div className="flex-1"><strong>{policy.relative_path || 'Raiz TCloud'}</strong><small>{policy.mode==='offline'?'Sempre offline':policy.mode==='online'?'Somente online':'Normal'} · atualizado em {new Date(policy.updated_at*1000).toLocaleString()}</small></div><button onClick={async()=>{await setFolderSyncPolicy(policy.relative_path,'normal');await refresh();}}>Remover</button></div>)}</div>
        </Feature>
      );
    }
    if (tab === 'conflicts') {
      return (
        <Feature title="Versões diferentes" description="O mesmo arquivo mudou no computador e na nuvem. Escolha qual versão usar ou guarde as duas para não perder nada.">
          <MetricGrid values={[[ 'Conflitos pendentes', syncConflicts.length ], [ 'Estratégia padrão', maturitySettings.conflict_strategy==='keep_both'?'Preservar ambos':'Perguntar' ]]} />
          <div className="tcloud-pro-actions"><select className="tcloud-pro-input" value={maturitySettings.conflict_strategy} onChange={(event)=>{const next={...maturitySettings,conflict_strategy:event.target.value as 'keep_both'|'ask'};setMaturitySettings(next);void setSyncMaturitySettings(next).catch(()=>{});}}><option value="keep_both">Preservar ambos por padrão</option><option value="ask">Sempre perguntar</option></select><Action icon={RefreshCw} onClick={refresh}>Atualizar</Action></div>
          <div className="tcloud-pro-list">{syncConflicts.length===0?<Empty text="Nenhum conflito pendente."/>:syncConflicts.map((conflict)=><div className="tcloud-pro-list-row" key={conflict.id}><div className="flex-1"><strong>{conflict.file_name || `Arquivo #${conflict.file_id}`}</strong><small>Computador: {formatBytesCompact(conflict.local_size??0)} · Nuvem: {conflict.remote_name || 'outra versão'} {conflict.remote_size!=null?`· ${formatBytesCompact(conflict.remote_size)}`:''}</small></div><button onClick={async()=>{await resolveSyncConflict(conflict.id,'keep_local');await refresh();toast.success('Versão do computador escolhida.');}}>Usar computador</button><button onClick={async()=>{await resolveSyncConflict(conflict.id,'keep_remote');await refresh();toast.success('Versão da nuvem escolhida.');}}>Usar nuvem</button><button onClick={async()=>{await resolveSyncConflict(conflict.id,'keep_both');await refresh();toast.success('As duas versões serão preservadas.');}}>Guardar as duas</button></div>)}</div>
        </Feature>
      );
    }
    if (tab === 'trash') {
      return (
        <Feature title="Lixeira do TCloud" description="Excluir pela interface agora é não destrutivo. Os arquivos ficam ocultos até você restaurar ou apagar definitivamente.">
          <div className="tcloud-pro-actions">
            <label className="text-metadata text-app-text-secondary">Excluir definitivamente após</label>
            <select className="tcloud-pro-input" value={trashRetentionDays} onChange={(event) => { const days = Number(event.target.value); setTrashRetentionDays(days); localStorage.setItem('tcloud:trash-retention-days', String(days)); const next={...maturitySettings,trash_retention_days:days};setMaturitySettings(next);void setSyncMaturitySettings(next).catch(()=>{}); }}>
              <option value={7}>7 dias</option><option value={30}>30 dias</option><option value={60}>60 dias</option><option value={90}>90 dias</option><option value={0}>Nunca automaticamente</option>
            </select>
          </div>
          <div className="tcloud-pro-list">
            {trash.length === 0 ? <Empty text="A Lixeira está vazia." /> : trash.map((item) => (
              <div className="tcloud-pro-list-row" key={item.id}>
                <div className="min-w-0 flex-1"><strong>{item.name}</strong><small>{formatBytesCompact(item.size)} · {new Date(item.deleted_at * 1000).toLocaleString()}</small></div>
                <button onClick={() => void restoreItem(item)}>Restaurar</button>
                <button className="is-danger" disabled={busy} onClick={() => void purgeItem(item)}>Excluir</button>
              </div>
            ))}
          </div>
          {trash.length > 0 && <Action onClick={async () => { if (!window.confirm('Esvaziar a Lixeira e agendar exclusão definitiva desses arquivos no Telegram?')) return; await purgeTrash(null); window.dispatchEvent(new Event('tcloud-trash-changed')); await refresh(); toast.success('Exclusões definitivas adicionadas à fila.'); }} icon={Trash2}>Esvaziar Lixeira</Action>}
        </Feature>
      );
    }
    if (tab === 'search') {
      return (
        <Feature title="Busca e filtros avançados" description="Use comandos junto com palavras para refinar instantaneamente a biblioteca carregada.">
          <input className="tcloud-pro-input" value={searchQuery} onChange={(event) => setSearchQuery(event.target.value)} placeholder="Ex.: relatório type:pdf size:>10MB before:2026-08-01" />
          <div className="tcloud-pro-chip-row"><code>type:pdf</code><code>type:image</code><code>size:&gt;100MB</code><code>before:2026-08-01</code><code>after:2026-01-01</code></div>
          <Action onClick={() => { onSearch(searchQuery); onClose(); }} icon={Search}>Pesquisar</Action>
        </Feature>
      );
    }
    if (tab === 'details') {
      return (
        <Feature title="Painel de detalhes" description="Informações técnicas e ações sobre o arquivo selecionado.">
          {!primaryFile ? <Empty text="Selecione um arquivo na grade." /> : <div className="tcloud-pro-details">
            <Row label="Nome" value={primaryFile.name} /><Row label="Tamanho" value={primaryFile.sizeStr} /><Row label="Mensagem Telegram" value={`#${primaryFile.id}`} /><Row label="Pasta" value={String(activeFolderId ?? 'Mensagens Salvas')} /><Row label="Data" value={primaryFile.created_at || '—'} /><Row label="Criptografia" value={primaryFile.encryption_state || 'Padrão'} />
          </div>}
        </Feature>
      );
    }
    if (tab === 'move') {
      return (
        <Feature title="Mover ou copiar entre pastas e fóruns" description="Copiar preserva a mensagem original; mover encaminha para o destino e remove a original.">
          <select className="tcloud-pro-input" value={targetFolderId ?? ''} onChange={(event) => setTargetFolderId(event.target.value ? Number(event.target.value) : null)}>
            <option value="">Mensagens Salvas</option>{folders.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}</option>)}
          </select>
          <Hint>{selection.length} arquivo(s) selecionado(s).</Hint>
          <div className="tcloud-pro-actions"><Action onClick={() => void moveOrCopy('copy')} icon={Copy}>Copiar</Action><Action onClick={() => void moveOrCopy('move')} icon={MoveRight}>Mover</Action></div>
        </Feature>
      );
    }
    if (tab === 'rules') {
      return (
        <Feature title="Organização automática por tipo" description="Crie regras para decidir em qual pasta cada extensão deve ser organizada.">
          <div className="tcloud-pro-form-grid"><input className="tcloud-pro-input" value={ruleName} onChange={(event) => setRuleName(event.target.value)} placeholder="Nome da regra" /><input className="tcloud-pro-input" value={ruleExtensions} onChange={(event) => setRuleExtensions(event.target.value)} placeholder="Extensões: jpg,png (vazio = qualquer)" /><input className="tcloud-pro-input" value={ruleNameContains} onChange={(event) => setRuleNameContains(event.target.value)} placeholder="Nome contém (opcional)" /><input className="tcloud-pro-input" inputMode="decimal" value={ruleMinMb} onChange={(event) => setRuleMinMb(event.target.value)} placeholder="Tamanho mín. MB" /><input className="tcloud-pro-input" inputMode="decimal" value={ruleMaxMb} onChange={(event) => setRuleMaxMb(event.target.value)} placeholder="Tamanho máx. MB" /><select className="tcloud-pro-input" value={ruleTarget ?? ''} onChange={(event) => setRuleTarget(event.target.value ? Number(event.target.value) : null)}><option value="">Mensagens Salvas</option>{folders.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}</option>)}</select></div>
          <div className="tcloud-pro-actions">
            <Action onClick={() => void createRule()} icon={Plus}>Adicionar regra</Action>
            <Action onClick={async () => {
              if (!selection.length) { toast.info('Selecione arquivos para organizar.'); return; }
              let moved = 0;
              for (const file of selection) {
                const ext = file.name.includes('.') ? file.name.split('.').pop()?.toLowerCase() || '' : '';
                const lowerName = file.name.toLowerCase();
                const rule = rules.find((candidate) => {
                  if (!candidate.enabled) return false;
                  const extensions = candidate.extensions.split(',').map((value) => value.trim().replace(/^\./, '').toLowerCase()).filter(Boolean);
                  const extensionMatches = extensions.length === 0 || extensions.includes(ext);
                  const nameMatches = !candidate.name_contains || lowerName.includes(candidate.name_contains.toLowerCase());
                  const minMatches = candidate.min_size == null || file.size >= candidate.min_size;
                  const maxMatches = candidate.max_size == null || file.size <= candidate.max_size;
                  return extensionMatches && nameMatches && minMatches && maxMatches;
                });
                if (!rule || rule.target_folder_id === activeFolderId) continue;
                try {
                  await invoke('cmd_move_files', { messageIds: [file.id], sourceFolderId: activeFolderId, targetFolderId: rule.target_folder_id });
                  moved += 1;
                } catch (error) { console.warn('Auto organize failed', file.name, error); }
              }
              if (moved > 0) { onSelectionChange([]); onRefresh(); toast.success(`${moved} arquivo(s) organizados.`); }
              else toast.info('Nenhum arquivo selecionado corresponde às regras ativas.');
            }} icon={WandSparkles}>Organizar selecionados agora</Action>
          </div>
          <div className="tcloud-pro-list">{rules.map((rule) => <div key={rule.id} className="tcloud-pro-list-row"><div className="flex-1"><strong>{rule.name}</strong><small>{[
  rule.extensions ? `ext: ${rule.extensions}` : 'qualquer extensão',
  rule.name_contains ? `nome contém: ${rule.name_contains}` : '',
  rule.min_size != null ? `mín: ${formatBytesCompact(rule.min_size)}` : '',
  rule.max_size != null ? `máx: ${formatBytesCompact(rule.max_size)}` : '',
].filter(Boolean).join(' · ')} → {folders.find((folder) => folder.id === rule.target_folder_id)?.name || 'Mensagens Salvas'}</small></div><button onClick={async () => { await saveRule({ ...rule, enabled: !rule.enabled }); await refresh(); }}>{rule.enabled ? 'Ativa' : 'Pausada'}</button><button className="is-danger" onClick={async () => { await deleteRule(rule.id); await refresh(); }}>Excluir</button></div>)}</div>
        </Feature>
      );
    }
    if (tab === 'offline') {
      return (
        <Feature title="Disponível offline / liberar espaço" description="Marque arquivos importantes para permanência local e controle o cache quando precisar liberar espaço.">
          {!primaryFile ? <Empty text="Selecione um arquivo para alterar o modo offline." /> : <div className="tcloud-pro-actions"><Action onClick={async () => { await invoke('cmd_get_preview', { messageId: primaryFile.id, folderId: primaryFile.folder_id ?? activeFolderId }); await pinOffline(primaryFile.id, true); toast.success('Arquivo baixado para o cache e marcado para uso offline.'); }} icon={Pin}>Manter offline</Action><Action onClick={async () => { await pinOffline(primaryFile.id, false); await invoke('cmd_delete_preview_for_message', { messageId: primaryFile.id, folderId: primaryFile.folder_id ?? activeFolderId }).catch(() => {}); await invoke('cmd_delete_image_thumbnail', { messageId: primaryFile.id, folderId: primaryFile.folder_id ?? activeFolderId }).catch(() => {}); toast.success('Cópia local removida. O arquivo continua na nuvem.'); }} icon={Eraser}>Liberar espaço</Action></div>}
          <Hint>O placeholder continua visível no Explorer mesmo quando a cópia local é liberada.</Hint>
        </Feature>
      );
    }
    if (tab === 'versions') {
      return (
        <Feature title="Histórico de versões" description="Antes de substituir um arquivo editado, o TCloud preserva automaticamente a mensagem anterior no Telegram como uma versão recuperável.">
          <div className="tcloud-pro-actions"><label className="text-metadata text-app-text-secondary">Reter por arquivo</label><select className="tcloud-pro-input" value={maturitySettings.version_retention} onChange={(event)=>{const next={...maturitySettings,version_retention:Number(event.target.value)};setMaturitySettings(next);void setSyncMaturitySettings(next).catch(()=>{});}}><option value={0}>Desativado</option><option value={5}>5 versões</option><option value={10}>10 versões</option><option value={30}>30 versões</option><option value={50}>50 versões</option></select>{primaryFile && <Action onClick={async () => { await addVersion(primaryFile.id, primaryFile.folder_id ?? activeFolderId, primaryFile.name, primaryFile.size, 'Snapshot manual'); await refresh(); toast.success('Versão registrada.'); }} icon={History}>Registrar snapshot manual</Action>}</div>
          <div className="tcloud-pro-list">{versions.filter((version) => !primaryFile || version.name === primaryFile.name).slice(0, 50).map((version) => <div key={version.id} className="tcloud-pro-list-row"><div className="flex-1"><strong>{version.name}</strong><small>{formatBytesCompact(version.size)} · {new Date(version.created_at * 1000).toLocaleString()} · msg #{version.message_id}</small></div><button onClick={async () => { try { await copyTelegramFiles([version.message_id], version.folder_id, activeFolderId); onRefresh(); toast.success('Versão restaurada como uma nova cópia.'); } catch (error) { toast.error(`Não foi possível restaurar a versão: ${error}`); } }}>Restaurar</button></div>)}</div>
        </Feature>
      );
    }
    if (tab === 'share') {
      return (
        <Feature title="Compartilhamento por link" description="Links temporários com expiração, senha, limite de downloads e histórico de acesso.">
          <div className="tcloud-pro-actions">
            {!primaryFile ? <Empty text="Selecione um arquivo para criar um novo link." /> : <Action onClick={() => { onClose(); onOpenShare(primaryFile); }} icon={Share2}>Criar link de compartilhamento</Action>}
            <Action icon={RefreshCw} onClick={() => void refresh()}>Atualizar histórico</Action>
          </div>
          <div className="tcloud-pro-list">
            {shareHistory.length === 0 ? <Empty text="Nenhum link de compartilhamento registrado." /> : shareHistory.slice(0, 50).map((share) => {
              const max = share.max_downloads ? `${share.download_count ?? 0}/${share.max_downloads}` : `${share.download_count ?? 0}/∞`;
              const state = share.revoked ? 'Revogado' : share.expires_at && share.expires_at * 1000 < Date.now() ? 'Expirado' : 'Ativo';
              return <div className="tcloud-pro-list-row" key={share.id}><div className="flex-1"><strong>{share.file_name}</strong><small>{state} · downloads {max} · criado em {new Date(share.created_at * 1000).toLocaleString()}{share.last_access_at ? ` · último acesso ${new Date(share.last_access_at * 1000).toLocaleString()}` : ''}</small></div></div>;
            })}
          </div>
          <h4 className="mt-4 font-semibold">Auditoria de acessos</h4>
          <div className="tcloud-pro-list">{shareAudit.length===0?<Empty text="Nenhum acesso auditado ainda."/>:shareAudit.slice(0,50).map((item)=><div className="tcloud-pro-list-row" key={item.id}><div className="flex-1"><strong>{item.ip || 'IP não informado'}</strong><small>{new Date(item.accessed_at*1000).toLocaleString()} · {item.authorized?'AUTORIZADO':'NEGADO'} · {item.range_request?'stream/range':'download/autenticação'} · {item.user_agent || 'cliente desconhecido'}</small></div></div>)}</div>
          <Hint>Downloads completos incrementam o contador; acessos Range de players ficam registrados separadamente na auditoria.</Hint>
        </Feature>
      );
    }
    if (tab === 'backup') {
      return (
        <Feature title="Backup automático de pastas do computador" description="Adicione origens locais. O TCloud verifica o agendamento em segundo plano enquanto o aplicativo estiver rodando e envia os arquivos para a pasta escolhida.">
          <div className="tcloud-pro-actions"><select className="tcloud-pro-input" value={targetFolderId ?? ''} onChange={(event) => setTargetFolderId(event.target.value ? Number(event.target.value) : null)}><option value="">Mensagens Salvas</option>{folders.map((folder) => <option key={folder.id} value={folder.id}>{folder.name}</option>)}</select><select className="tcloud-pro-input" value={backupInterval} onChange={(event) => setBackupInterval(Number(event.target.value))}><option value={60}>A cada hora</option><option value={360}>A cada 6 horas</option><option value={1440}>Uma vez por dia</option></select><Action onClick={() => void addBackupSource()} icon={Plus}>Adicionar pasta</Action><Action onClick={()=>{const next=!autoCompressBatches;setAutoCompressBatches(next);localStorage.setItem('tcloud:auto-compress-batches',String(next));toast.success(next?'Compactação automática de lotes ativada.':'Compactação automática de lotes desativada.');}} icon={ArchiveRestore}>{autoCompressBatches?'Compactação de lotes: ON':'Compactação de lotes: OFF'}</Action></div>
          <Hint>Quando ativada, a seleção de vários arquivos dentro de uma pasta/tópico é compactada em um único ZIP antes do envio. Upload na raiz continua separado para respeitar regras automáticas de organização.</Hint>
          <div className="tcloud-pro-list">{backups.map((backup) => <div key={backup.id} className="tcloud-pro-list-row"><div className="min-w-0 flex-1"><strong>{backup.local_path}</strong><small>A cada {backup.interval_minutes} min · {backup.last_scan_at
  ? `Verificado: ${new Date(backup.last_scan_at * 1000).toLocaleString()}`
  : 'Ainda não verificado'}
{' · '}
{backup.last_success_at
  ? `Backup concluído: ${new Date(backup.last_success_at * 1000).toLocaleString()}`
  : 'Nenhum backup concluído'}</small></div><button onClick={async () => { const result = await scanBackup(backup.id); toast.info(`${result.files_found} arquivos · ${formatBytesCompact(result.total_bytes)}`); await refresh(); }}>Analisar</button><button onClick={() => void onRunBackup(backup)}>Executar</button><button className="is-danger" onClick={async () => { await deleteBackup(backup.id); await refresh(); }}>Excluir</button></div>)}</div>
        </Feature>
      );
    }
    if (tab === 'gallery') {
      return <Feature title="Galeria de fotos" description="Visualização dedicada para fotos, usando miniaturas em cache e o visualizador completo ao abrir."><TCloudGallery files={files} onOpen={(file) => { onSelectionChange([file.id]); onClose(); window.dispatchEvent(new CustomEvent('tcloud-open-preview-id',{detail:{id:file.id}})); }} /></Feature>;
    }
    if (tab === 'content') {
      return <Feature title="Busca por conteúdo, PDF e OCR" description="Baixa sob demanda os documentos/imagens indexáveis, extrai texto de PDFs e usa OCR local do Windows. A interface continua responsiva porque os downloads entram na fila do Sync.">
        <div className="tcloud-pro-actions"><Action icon={CloudDownload} onClick={async()=>{if(!window.confirm('A indexação completa precisa baixar conteúdo que ainda está somente na nuvem. Continuar com um lote de até 250 arquivos?'))return;const result=await queueIndexableDownloads(250);toast.success(`${result.queued} documento(s)/imagem(ns) adicionados à fila para indexação. ${result.already_queued} já estavam na fila.`);}}>Baixar biblioteca indexável</Action><Action icon={ScanText} onClick={async()=>{setBusy(true);try{const result=await reindexFullContent(true);setContentIndexResult(result);toast.success(`${result.text_files} texto(s), ${result.pdf_files} PDF(s) e ${result.ocr_images} imagem(ns) indexados.`);}finally{setBusy(false);}}}>{busy?'Indexando…':'Indexar texto + PDF + OCR'}</Action></div>
        {contentIndexResult&&<MetricGrid values={[[ 'Textos',contentIndexResult.text_files ],[ 'PDFs',contentIndexResult.pdf_files ],[ 'OCR',contentIndexResult.ocr_images ],[ 'Ignorados',contentIndexResult.skipped ]]}/>} 
        {contentIndexResult?.errors?.length ? <details className="tcloud-error-row"><summary>{contentIndexResult.errors.length} aviso(s) de indexação</summary><pre>{contentIndexResult.errors.join('\n')}</pre></details>:null}
        <div className="tcloud-pro-actions"><input className="tcloud-pro-input" value={contentQuery} onChange={(e)=>setContentQuery(e.target.value)} placeholder="Texto dentro do PDF, imagem ou arquivo..."/><Action icon={Search} onClick={async()=>setContentHits(await searchIndexedContent(contentQuery))}>Pesquisar</Action></div>
        <div className="tcloud-pro-list">{contentHits.map((hit)=><div className="tcloud-pro-list-row" key={hit.file_id}><div className="min-w-0 flex-1"><strong>{hit.name}</strong><small>{formatBytesCompact(hit.size)} · {hit.snippet || 'Correspondência no nome do arquivo'}</small></div></div>)}</div>
      </Feature>;
    }
    if (tab === 'smart') {
      const smart = [
        ['Arquivos grandes','size:>100MB'],['Vídeos','type:video'],['PDFs','type:pdf'],['Últimos 7 dias','after:'+new Date(Date.now()-7*86400000).toISOString().slice(0,10)],['Imagens','type:image'],['Áudio','type:audio']
      ] as Array<[string,string]>;
      return <Feature title="Pastas inteligentes" description="Coleções virtuais: não movem os arquivos. Elas aplicam filtros sobre a biblioteca atual."><div className="tcloud-smart-grid">{smart.map(([label,query])=><button key={label} onClick={()=>{onSearch(query);onClose();}}><Folder className="h-5 w-5"/><strong>{label}</strong><small>{query}</small></button>)}</div></Feature>;
    }
    if (tab === 'duplicates') {
      return <Feature title="Detector de duplicados" description="Usa SHA-256 para confirmar conteúdo idêntico e também pode verificar o arquivo local antes do upload."><label className="tcloud-switch-row"><input type="checkbox" checked={duplicateCheck} onChange={(e)=>{setDuplicateCheck(e.target.checked);localStorage.setItem('tcloud:duplicate-check-before-upload',String(e.target.checked));}}/><span><strong>Verificar antes do upload</strong><small>Se um hash idêntico já estiver no índice, o TCloud pergunta antes de reenviar.</small></span></label><MetricGrid values={[[ 'Grupos', duplicates.length ], [ 'Espaço potencial', formatBytesCompact(duplicates.reduce((n,g)=>n+g.reclaimable_bytes,0)) ]]}/><div className="tcloud-pro-list">{duplicates.map((group)=><div className="tcloud-duplicate-group" key={group.sha256}><strong>{group.files.length} cópias · {formatBytesCompact(group.size)}</strong><small>{group.sha256.slice(0,20)}…</small>{group.files.map((file)=><span key={file.file_id}>{file.name}</span>)}</div>)}</div></Feature>;
    }
    if (tab === 'cleanup') {
      return <Feature title="Limpeza inteligente" description="Executa a limpeza do cache antigo não fixado, remove metadados de versões antigas e expira registros locais da Lixeira sem apagar silenciosamente o arquivo remoto."><MetricGrid values={[[ 'Cache', formatBytesCompact(cleanupStats?.cached_bytes??0) ],[ 'Duplicados recuperáveis', formatBytesCompact(cleanupStats?.duplicate_reclaimable_bytes??0) ],[ 'Lixeira', cleanupStats?.trash_items??0 ],[ 'Versões', cleanupStats?.versions??0 ]]}/><div className="tcloud-pro-actions"><Action icon={Eraser} onClick={async()=>{const result=await applySmartCleanup(30,trashRetentionDays,90);toast.success(`Limpeza concluída: ${formatBytesCompact(result.reclaimed_bytes)} liberados, ${result.removed_files} arquivo(s) de cache removidos.`);await refresh();}}>Executar limpeza recomendada</Action><Action icon={RefreshCw} onClick={refresh}>Recalcular</Action><Action icon={Sparkles} onClick={()=>{const next=!autoCleanup;setAutoCleanup(next);localStorage.setItem('tcloud:auto-cleanup',String(next));toast.success(next?'Limpeza automática diária ativada.':'Limpeza automática desativada.');}}>{autoCleanup?'Automática: ON':'Automática: OFF'}</Action></div></Feature>;
    }
    if (tab === 'activity') {
      return <Feature title="Atividades recentes" description="Histórico local de operações importantes do TCloud."><Action icon={RefreshCw} onClick={refresh}>Atualizar</Action><div className="tcloud-pro-list">{activities.length===0?<Empty text="Nenhuma atividade registrada ainda."/>:activities.map((item)=><div className="tcloud-pro-list-row" key={item.id}><div className="flex-1"><strong>{item.title}</strong><small>{new Date(item.created_at*1000).toLocaleString()} · {item.detail || item.kind}</small></div></div>)}</div></Feature>;
    }
    if (tab === 'health') {
      return <Feature title="Saúde e diagnóstico profundo" description="Testa SQLite, sessão Telegram, cache, FFmpeg, CFAPI, Windows Hello e espaço em disco em uma única execução."><Action icon={HeartPulse} onClick={async()=>{setBusy(true);try{setDiagnostics(await runDeepDiagnostics());}finally{setBusy(false);}}}>{busy?'Testando…':'Verificar TCloud'}</Action><div className="tcloud-pro-list">{diagnostics.length===0?<Empty text="Clique em Verificar TCloud para executar os testes."/>:diagnostics.map((check)=><div className="tcloud-pro-list-row" key={check.name}><div className="flex-1"><strong>{check.name} · {check.status.toUpperCase()}</strong><small>{check.detail}</small></div></div>)}</div></Feature>;
    }
    if (tab === 'notifications') {
      return <Feature title="Notificações do Windows" description="O TCloud pode avisar sobre backup, erros e sincronização sem precisar ficar com a janela aberta."><Action icon={Bell} onClick={async()=>{await sendDesktopNotification('TCloud','Notificações do TCloud estão funcionando.');toast.success('Notificação de teste enviada.');}}>Enviar notificação de teste</Action><Hint>As notificações usam o sistema nativo do Windows quando disponível.</Hint></Feature>;
    }
    if (tab === 'tray') {
      return <Feature title="Bandeja do sistema" description="O ícone ao lado do relógio agora oferece abrir a pasta TCloud, upload rápido, pausar sincronização, status e encerrar."><div className="tcloud-pro-actions"><Action icon={RefreshCw} onClick={async()=>{const next=!syncPaused;await pauseAllJobs(next);setSyncPaused(next);localStorage.setItem('tcloud:sync-paused',String(next));await addActivity('sync',next?'Sincronização pausada':'Sincronização retomada');toast.success(next?'Sincronização pausada.':'Sincronização retomada.');}}>{syncPaused?'Retomar sincronização':'Pausar sincronização'}</Action></div></Feature>;
    }
    if (tab === 'updates') {
      return <Feature title="Atualizações com rollback" description="Além de verificar novas versões, o TCloud mantém uma cópia do executável anterior antes da instalação para recuperação manual caso uma atualização falhe."><MetricGrid values={[[ 'Rollback preparado',rollback?.prepared?'Sim':'Não' ],[ 'Versão alvo',rollback?.target_version||'—' ],[ 'Build saudável',rollback?.healthy?'Sim':'Aguardando' ]]}/><div className="tcloud-pro-actions"><Action icon={Rocket} onClick={async()=>{try{const update=await check();if(update){toast.success(`TCloud ${update.version} disponível. Use o banner de atualização para instalar com backup automático.`);}else{toast.success('Você já está na versão mais recente.');}}catch(error){toast.error(`Não foi possível verificar: ${error}`);}}}>Verificar atualização</Action>{rollback?.prepared&&<Action icon={ArchiveRestore} onClick={async()=>{if(!window.confirm('Restaurar o executável anterior do TCloud?'))return;await forceRollback();toast.info('Rollback agendado. Encerre o TCloud para concluir.');}}>Restaurar versão anterior</Action>}</div></Feature>;
    }
    if (tab === 'onboarding') {
      return <Feature title="Primeira configuração" description="Checklist rápido para deixar uma instalação nova pronta."><div className="tcloud-onboarding-checks"><div className="is-done"><CheckCircle2/>Telegram conectado</div><div className={cfapi?.provider_connected?'is-done':''}><CheckCircle2/>Explorer CFAPI {cfapi?.provider_connected?'conectado':'pendente'}</div><div className={(stats?.files??0)>0?'is-done':''}><CheckCircle2/>Índice local {(stats?.files??0)>0?'pronto':'aguardando'}</div><div className={backups.length>0?'is-done':''}><CheckCircle2/>Backup {backups.length>0?'configurado':'opcional'}</div></div></Feature>;
    }
    if (tab === 'profiles') {
      return <Feature title="Perfis de desempenho" description="Ajustam simultaneamente miniaturas e quantidade de transferências concorrentes."><div className="tcloud-profile-grid">{(['economy','balanced','performance'] as TCloudPerformanceProfile[]).map((item)=><button className={profile===item?'is-active':''} key={item} onClick={async()=>{await applyPerformanceProfile(item);setProfile(item);toast.success(`Perfil ${item} aplicado.`);}}><Gauge className="h-5 w-5"/><strong>{item==='economy'?'Economia':item==='balanced'?'Balanceado':'Desempenho'}</strong><small>{item==='economy'?'Menos CPU/rede':item==='balanced'?'Uso recomendado':'Mais miniaturas e transferências'}</small></button>)}</div></Feature>;
    }
    if (tab === 'power') {
      return <Feature title="Economia de bateria e rede" description="Quando ativado, notebooks podem entrar automaticamente no perfil Economia ao cair abaixo de 35% fora da tomada."><label className="tcloud-switch-row"><input type="checkbox" checked={autoEconomy} onChange={(e)=>{setAutoEconomy(e.target.checked);setAutoEconomyEnabled(e.target.checked);}}/><span><strong>Economia automática</strong><small>Reduz miniaturas simultâneas e transferências concorrentes.</small></span></label></Feature>;
    }
    if (tab === 'errors') {
      return <Feature title="Erros e diagnóstico" description="Erros técnicos ficam aqui em vez de poluir a interface principal. O pacote de diagnóstico exclui credenciais e sessão Telegram."><div className="tcloud-pro-actions"><Action icon={Trash2} onClick={()=>{clearClientErrors();setClientErrors([]);}}>Limpar erros</Action><Action icon={FileDown} onClick={async()=>{const {save}=await import('@tauri-apps/plugin-dialog');const path=await save({title:'Exportar diagnóstico TCloud',defaultPath:`TCloud-diagnostico-${new Date().toISOString().slice(0,10)}.zip`,filters:[{name:'ZIP',extensions:['zip']} ]});if(!path)return;await exportDiagnostics(path);toast.success('Diagnóstico exportado.');}}>Exportar diagnóstico</Action></div><div className="tcloud-pro-list">{clientErrors.length===0?<Empty text="Nenhum erro de interface registrado."/>:clientErrors.map((error)=><details className="tcloud-error-row" key={error.id}><summary><strong>{error.message}</strong><small>{new Date(error.createdAt).toLocaleString()} · {error.source || 'TCloud'}</small></summary>{error.stack&&<pre>{error.stack}</pre>}</details>)}</div></Feature>;
    }
    if (tab === 'metadata') {
      return <Feature title="Tags e notas" description="Metadados locais para organizar arquivos sem precisar mudar o nome ou mover o conteúdo no Telegram.">{!primaryFile?<Empty text="Selecione um arquivo para adicionar tags e uma descrição."/>:<><div className="tcloud-pro-details"><Row label="Arquivo" value={primaryFile.name}/><Row label="Atualizado" value={fileMetadata?.updated_at ? new Date(fileMetadata.updated_at*1000).toLocaleString() : 'Ainda não salvo'}/></div><input className="tcloud-pro-input" value={metadataTags} onChange={(e)=>setMetadataTags(e.target.value)} placeholder="Tags separadas por vírgula: Faculdade, Importante, Backup"/><textarea className="tcloud-pro-input min-h-28" value={metadataNote} onChange={(e)=>setMetadataNote(e.target.value)} placeholder="Descrição ou anotação sobre o arquivo..."/><Action icon={Tag} onClick={async()=>{const value=await setFileMetadata(primaryFile.id,primaryFile.folder_id??activeFolderId,metadataTags.split(',').map(v=>v.trim()).filter(Boolean),metadataNote);setFileMetadataState(value);toast.success('Tags e notas salvas.');}}>Salvar metadados</Action></>}</Feature>;
    }
    if (tab === 'storage') {
      const total=storageBuckets.reduce((sum,item)=>sum+item.bytes,0);
      return <Feature title="Mapa de armazenamento" description="Distribuição do índice local por categoria para descobrir rapidamente o que mais ocupa espaço."><MetricGrid values={[[ 'Total indexado', formatBytesCompact(total) ],[ 'Categorias', storageBuckets.filter(item=>item.files>0).length ]]}/><div className="tcloud-storage-map">{storageBuckets.map((item)=>{const pct=total>0?(item.bytes/total)*100:0;return <div className="tcloud-storage-map-row" key={item.category}><div><strong>{item.category}</strong><small>{item.files} arquivo(s) · {formatBytesCompact(item.bytes)}</small></div><div className="tcloud-storage-bar"><span style={{width:`${Math.max(pct,item.bytes>0?1:0)}%`}}/></div><b>{pct.toFixed(1)}%</b></div>;})}</div></Feature>;
    }
    if (tab === 'config') {
      return <Feature title="Backup das configurações" description="Exporta preferências do TCloud para JSON e permite restaurá-las em outra instalação. Credenciais e sessão Telegram não são incluídas."><div className="tcloud-pro-actions"><Action icon={FileDown} onClick={async()=>{const {save}=await import('@tauri-apps/plugin-dialog');const store=await load('settings.json');const settingsValue=await store.get('settings');const localEntries:Record<string,string|null>=Object.fromEntries(Array.from({length:localStorage.length},(_,i)=>localStorage.key(i)).filter((key): key is string => Boolean(key&&key.startsWith('tcloud:'))).map(key=>[key,localStorage.getItem(key)]));const payload=JSON.stringify({version:10,settings:settingsValue,localStorage:localEntries},null,2);const path=await save({title:'Exportar configurações TCloud',defaultPath:'TCloud-configuracoes.json',filters:[{name:'JSON',extensions:['json']}]});if(!path)return;await exportConfigJson(path,payload);toast.success('Configurações exportadas.');}}>Exportar configurações</Action><Action icon={ArchiveRestore} onClick={async()=>{const {open}=await import('@tauri-apps/plugin-dialog');const picked=await open({multiple:false,directory:false,filters:[{name:'JSON',extensions:['json']}]});if(!picked||Array.isArray(picked))return;const raw=await importConfigJson(picked);const parsed=JSON.parse(raw);if(parsed.settings){const store=await load('settings.json');await store.set('settings',parsed.settings);await store.save();}if(parsed.localStorage&&typeof parsed.localStorage==='object'){Object.entries(parsed.localStorage).forEach(([key,value])=>{if(key.startsWith('tcloud:')&&typeof value==='string')localStorage.setItem(key,value);});}toast.success('Configurações restauradas. Reiniciando interface...');window.setTimeout(()=>window.location.reload(),500);}}>Importar configurações</Action></div><Hint>API ID/hash, senhas, chaves criptográficas e arquivo de sessão não fazem parte do backup.</Hint></Feature>;
    }
    if (tab === 'security') {
      return <Feature title="Cofre, Windows Hello e criptografia por pasta" description="Proteção criptográfica integrada ao upload, com política por pasta e desbloqueio opcional via Windows Hello.">
        <MetricGrid values={[[ 'Cofre',vaultStatus?.exists?(vaultStatus.is_unlocked?'Desbloqueado':'Bloqueado'):'Não criado' ],[ 'Windows Hello',hello?.supported?(hello.enrolled?'Configurado':'Disponível'):'Indisponível' ],[ 'Pasta atual',activeFolderId===null?'Mensagens Salvas':String(activeFolderId) ]]}/>
        <input className="tcloud-pro-input" type="password" value={vaultPassphrase} onChange={(e)=>setVaultPassphrase(e.target.value)} placeholder="Senha do Cofre (não é salva em texto puro)"/>
        <div className="tcloud-pro-actions">{!vaultStatus?.exists?<Action icon={LockKeyhole} onClick={async()=>{if(vaultPassphrase.length<8){toast.error('Use pelo menos 8 caracteres.');return;}await invoke('cmd_create_vault',{passphrase:vaultPassphrase});setVaultPassphrase('');await refresh();toast.success('Cofre criado e desbloqueado.');}}>Criar Cofre</Action>:vaultStatus?.is_unlocked?<Action icon={LockKeyhole} onClick={async()=>{await invoke('cmd_lock_vault');await refresh();}}>Bloquear Cofre</Action>:<Action icon={LockKeyhole} onClick={async()=>{await invoke('cmd_unlock_vault',{passphrase:vaultPassphrase});setVaultPassphrase('');await refresh();}}>Desbloquear</Action>}{hello?.supported&&!hello.enrolled&&vaultStatus?.exists&&<Action icon={ShieldCheck} onClick={async()=>{if(!vaultPassphrase){toast.info('Digite a senha atual do Cofre primeiro.');return;}await invoke('cmd_unlock_vault',{passphrase:vaultPassphrase});setHello(await enrollWindowsHello(vaultPassphrase));setVaultPassphrase('');await refresh();toast.success('Windows Hello configurado e vinculado ao Cofre.');}}>Ativar Windows Hello</Action>}{hello?.enrolled&&<Action icon={ShieldCheck} onClick={async()=>{await unlockWithWindowsHello();await refresh();toast.success('Cofre desbloqueado com Windows Hello.');}}>Desbloquear com Hello</Action>}{hello?.enrolled&&<Action icon={X} onClick={async()=>setHello(await removeWindowsHello())}>Remover Hello</Action>}</div>
        {activeFolderId!==null&&<><h4 className="mt-4 font-semibold">Política da pasta atual</h4><div className="tcloud-pro-actions"><select className="tcloud-pro-input" value={folderCrypto?.mode||'inherit'} onChange={async(e)=>setFolderCrypto(await setFolderCryptoPolicy(activeFolderId,e.target.value as FolderCryptoPolicy['mode'],folderCrypto?.protect_metadata??true))}><option value="inherit">Herdar configuração global</option><option value="standard">Criptografia padrão</option><option value="vault">Sempre usar Cofre</option></select><label className="tcloud-switch-row"><input type="checkbox" checked={folderCrypto?.protect_metadata??true} onChange={async(e)=>setFolderCrypto(await setFolderCryptoPolicy(activeFolderId,folderCrypto?.mode||'inherit',e.target.checked))}/><span><strong>Proteger metadados</strong></span></label></div></>}
      </Feature>;
    }
    if (tab === 'music') {
      const artists=new Set(musicTracks.map(t=>t.artist)); const albums=new Set(musicTracks.map(t=>t.album));
      return <Feature title="Biblioteca de música" description="Organiza áudio por artista/álbum quando o FFprobe consegue ler as tags e permite criar playlists persistentes."><MetricGrid values={[[ 'Faixas',musicTracks.length ],[ 'Artistas',artists.size ],[ 'Álbuns',albums.size ],[ 'Playlists',musicPlaylists.length ]]}/><div className="tcloud-pro-actions"><input className="tcloud-pro-input" value={playlistName} onChange={(e)=>setPlaylistName(e.target.value)} placeholder="Nome da playlist"/><Action icon={Plus} onClick={async()=>{const ids=musicTracks.filter(t=>t.message_id!==null&&selectedIds.includes(t.message_id!)).map(t=>t.file_id);if(!ids.length){toast.info('Selecione faixas na biblioteca principal primeiro.');return;}await saveMusicPlaylist(null,playlistName,ids);await refresh();toast.success('Playlist criada.');}}>Criar com selecionadas</Action></div><div className="tcloud-pro-list">{musicTracks.slice(0,300).map(track=><div className="tcloud-pro-list-row" key={track.file_id}><div className="flex-1"><strong>{track.title}</strong><small>{track.artist} · {track.album}{track.duration_seconds?` · ${Math.floor(track.duration_seconds/60)}:${String(Math.floor(track.duration_seconds%60)).padStart(2,'0')}`:''}{track.cached?' · cache':''}</small></div>{track.message_id!==null&&<button onClick={()=>{onClose();window.dispatchEvent(new CustomEvent('tcloud-open-preview-id',{detail:{id:track.message_id}}));}}>Tocar</button>}</div>)}</div>{musicPlaylists.map(pl=>{const playlistTracks=pl.file_ids.map(id=>musicTracks.find(track=>track.file_id===id)).filter((track): track is MusicTrack=>Boolean(track&&track.message_id!==null));const contextIds=playlistTracks.map(track=>track.message_id!);return <div className="tcloud-pro-list-row" key={pl.id}><div className="flex-1"><strong>Playlist: {pl.name}</strong><small>{pl.file_ids.length} faixa(s) · navegação anterior/próxima no player</small></div>{contextIds.length>0&&<button onClick={()=>{onClose();window.dispatchEvent(new CustomEvent('tcloud-open-preview-id',{detail:{id:contextIds[0],contextIds}}));}}>Tocar playlist</button>}<button className="is-danger" onClick={async()=>{await deleteMusicPlaylist(pl.id);await refresh();}}>Excluir</button></div>;})}</Feature>;
    }
    if (tab === 'developer') {
      return <Feature title="Modo desenvolvimento" description="Ferramentas para testes sem deixar o tcloud.exe preso entre recompilações."><MetricGrid values={[[ 'Build', import.meta.env.DEV ? 'Desenvolvimento' : 'Release' ],[ 'Fechar no X', import.meta.env.DEV ? 'Encerra completamente' : 'Mantém na bandeja' ]]}/><label className="tcloud-switch-row"><input type="checkbox" checked={debugOverlayEnabled} onChange={(e)=>{setDebugOverlayEnabled(e.target.checked);localStorage.setItem('debug_overlay',e.target.checked?'1':'0');window.dispatchEvent(new Event('storage'));}}/><span><strong>Overlay técnico do player</strong><small>Mostra informações de buffer/transcode durante testes de vídeo.</small></span></label><Hint>Em `tauri dev`, fechar a janela encerra o processo para o Cargo conseguir recompilar. Em release, o comportamento de bandeja permanece.</Hint></Feature>;
    }
    return (
      <Feature title="TCloud no Explorer + CFAPI nativa" description="Integração principal com o Windows: placeholders, hidratação Telegram/TDENC2, offline/pinning, cancelamento e recuperação automática do Sync Root.">
        <MetricGrid values={[[ 'Provider CFAPI',cfapi?.provider_connected?'Conectado':cfapi?.registered?'Desconectado':cfapi?.platform_supported?'Disponível':'Indisponível' ], [ 'Explorer/Shell',cfapi?.shell_registered?'Registrado':cfapi?.registered?'Legado/pendente':'Inativo' ], [ 'Identidade Windows',cfapi?.package_identity?'Ativa':cfapi?.registered?'Opcional':'Ausente' ], [ 'Offline no Explorer',cfapi?.native_offline_menu?'Nativo do Windows':cfapi?.fallback_menu_installed?'Compatível TCloud':cfapi?.registered?'Pendente':'Inativo' ], [ 'Hidratação',cfapi?.hydration_ready?'Pronta':cfapi?.registered?'Aguardando provider':'Inativa' ], [ 'Criptografia',cfapi?.encrypted_hydration_ready?'Cofre pronto':'Cofre bloqueado/indisponível' ], [ 'Placeholders',String(cfapi?.placeholder_count ?? 0) ], [ 'Somente na nuvem',String(cfapi?.cloud_only_count ?? 0) ], [ 'No computador',String(cfapi?.cached_count ?? 0) ], [ 'Sempre offline',String(cfapi?.offline_pinned_count ?? 0) ], [ 'FETCH ativos',String(cfapi?.active_fetches ?? 0) ], [ 'Concluídos',String(cfapi?.completed_fetches ?? 0) ], [ 'Cancelados',String(cfapi?.cancelled_fetches ?? 0) ], [ 'Falhas',String(cfapi?.failed_fetches ?? 0) ], [ 'Operações pendentes',String(cfapi?.pending_operations ?? 0) ], [ 'Precisam de atenção',String(cfapi?.failed_operations ?? 0) ]]} />
        <div className="tcloud-pro-actions">
          {cfapi?.platform_supported&&<Action onClick={async()=>{try{setCfapi(await setCfapiEnabled(!cfapi.registered));toast.success(cfapi.registered?'CFAPI desativada.':'Sync Root CFAPI + provider ativados.');}catch(e){toast.error(String(e));}}} icon={CloudDownload}>{cfapi?.registered?'Desativar CFAPI':'Ativar CFAPI'}</Action>}
          {cfapi?.registered&&!cfapi.shell_pinning_enabled&&<Action onClick={async()=>{setBusy(true);try{const next=await finalizeCfapiExplorer();setCfapi(next);toast.success(next.shell_pinning_enabled?'Integração nativa do Explorer concluída.':'Migração executada; confira o diagnóstico do Shell.');}catch(e){toast.error(String(e));}finally{setBusy(false);}}} icon={Wrench}>{busy?'Finalizando…':'Finalizar integração do Explorer'}</Action>}
          {cfapi?.registered&&<Action onClick={async()=>{setBusy(true);try{const r=await checkCfapiIntegrity(true);setCfapi(await getCfapiStatus());if(r.missing_placeholders||r.errors.length){toast.warning(`${r.missing_placeholders} item(ns) ainda ausentes; ${r.errors.length} erro(s).`);}else{toast.success(r.repaired?`${r.repaired} item(ns) reparados.`:'Integração do Explorer íntegra.');}}catch(e){toast.error(String(e));}finally{setBusy(false);}}} icon={ShieldCheck}>{busy?'Verificando…':'Verificar integridade'}</Action>}
          {cfapi?.registered&&<Action onClick={async()=>{setBusy(true);try{const r=await populateCfapiPlaceholders();setCfapi(await getCfapiStatus());toast.success(`${r.created} placeholder(s) criado(s); ${r.existing} reparado(s)/já existente(s).`);if(r.errors.length) toast.warning(`${r.errors.length} item(ns) exigem revisão.`);}catch(e){toast.error(String(e));}finally{setBusy(false);}}} icon={RefreshCw}>{busy?'Processando…':'Sincronizar placeholders'}</Action>}
          {cfapi?.registered&&<Action onClick={async()=>{if(!window.confirm(`Manter localmente todos os ${cfapi.placeholder_count} placeholder(s) do TCloud? Isso pode iniciar muitos downloads.`))return;try{const r=await keepCfapiOnDevice();toast.success(r.detail);}catch(e){toast.error(String(e));}}} icon={Pin}>Manter tudo neste dispositivo</Action>}
          {cfapi?.registered&&<Action onClick={async()=>{if(!window.confirm(`Permitir que o Windows libere o conteúdo local dos ${cfapi.placeholder_count} placeholder(s) em sincronia? Os arquivos continuarão visíveis no TCloud.`))return;try{const r=await freeCfapiSpace();toast.success(r.detail);}catch(e){toast.error(String(e));}}} icon={Eraser}>Liberar espaço de tudo</Action>}
          {cfapi?.registered&&<Action onClick={async()=>{try{await openCfapiRoot();}catch(e){toast.error(String(e));}}} icon={Folder}>Abrir pasta CFAPI</Action>}
        </div>
        {cfapi?.registered&&<div className="tcloud-pro-list"><div className="tcloud-pro-list-row"><div className="flex-1"><strong>{cfapi.phase}</strong><small>{cfapi.root_path}{cfapi.shell_sync_root_id?` · ${cfapi.shell_sync_root_id}`:''}</small></div></div></div>}
        {cfapi?.registered&&cfapi.shell_pinning_enabled&&<Hint>Para manter ou liberar um arquivo individual, use o menu nativo do Explorer. Os botões “Manter tudo” e “Liberar espaço de tudo” abaixo da Central atuam recursivamente sobre toda a raiz TCloud.</Hint>}
        {cfapi?.registered&&!cfapi.package_identity&&<Hint>Package Identity não está ativa nesta execução. O TCloud usa o menu nativo do Windows quando disponível e só mantém o menu compatível como fallback quando necessário.</Hint>}
        {cfapi?.registered&&cfapi.package_identity&&!cfapi.shell_pinning_enabled&&<Hint>{cfapi.shell_detail || 'O provider CFAPI está ativo, mas o registro moderno do Explorer ainda precisa ser finalizado para expor as opções nativas de offline.'}</Hint>}
        {cfapi?.last_provider_error&&<Hint>Último erro do provider: {cfapi.last_provider_error}</Hint>}<Hint>{cfapi?.detail || 'CFAPI/Explorer é a integração principal do TCloud com o Windows. O Sync Root é recuperado automaticamente quando necessário.'}</Hint>
      </Feature>
    );
  })();

  const visibleTabs = showTechnicalTabs ? tabs : tabs.filter((item) => SIMPLE_ADVANCED_TABS.has(item.id));

  return (
    <div className="fixed inset-0 z-[290] flex items-center justify-center bg-app-overlay p-4 backdrop-blur-sm" onClick={onClose}>
      <div className="tcloud-pro-modal" onClick={(event) => event.stopPropagation()}>
        <header className="tcloud-pro-header"><div><span>TCLOUD</span><h2>{showTechnicalTabs ? 'Ferramentas avançadas' : 'Recursos avançados'}</h2><p>{showTechnicalTabs ? 'Diagnóstico, integração, desempenho e recursos técnicos.' : 'Recursos adicionais e ferramentas de manutenção do TCloud.'}</p></div><button onClick={onClose}><X className="h-4 w-4" /></button></header>
        <div className="tcloud-pro-layout">
          <nav className="tcloud-pro-nav">{visibleTabs.map(({ id, label, icon: Icon, number }) => <button key={id} className={tab === id ? 'is-active' : ''} onClick={() => setTab(id)}><span className="tcloud-pro-nav-number">{number}</span><Icon className="h-4 w-4" /><span>{label}</span></button>)}</nav>
          <main className="tcloud-pro-content">{content}</main>
        </div>
      </div>
    </div>
  );
}

function Feature({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return <section className="tcloud-pro-feature"><div className="tcloud-pro-feature-title"><CheckCircle2 className="h-5 w-5 text-app-accent" /><div><h3>{title}</h3><p>{description}</p></div></div><div className="tcloud-pro-feature-body">{children}</div></section>;
}
function MetricGrid({ values }: { values: Array<[string, string | number]> }) { return <div className="tcloud-pro-metrics">{values.map(([label, value]) => <div key={label}><span>{label}</span><strong>{value}</strong></div>)}</div>; }
function Action({ onClick, icon: Icon, children }: { onClick: () => void | Promise<void>; icon: typeof Play; children: ReactNode }) { return <button type="button" className="tcloud-pro-action" onClick={() => void onClick()}><Icon className="h-4 w-4" />{children}</button>; }
function Hint({ children }: { children: ReactNode }) { return <div className="tcloud-pro-hint"><ShieldCheck className="h-4 w-4" />{children}</div>; }
function Empty({ text }: { text: string }) { return <div className="tcloud-pro-empty">{text}</div>; }
function Row({ label, value }: { label: string; value: string }) { return <div className="tcloud-pro-detail-row"><span>{label}</span><strong>{value}</strong></div>; }
