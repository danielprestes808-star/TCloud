import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  AlertTriangle,
  ArchiveRestore,
  CheckCircle2,
  Database,
  Gauge,
  Pause,
  Play,
  RefreshCw,
  Timer,
  Trash2,
  X,
} from 'lucide-react';
import {
  cleanupTCloudCache,
  getTCloudCacheStatus,
  setTCloudCacheLimit,
  type TCloudCacheStatus,
} from '../../../services/tcloudCacheManager';
import {
  getTCloudNetworkSettings,
  setTCloudNetworkSettings,
  type TCloudNetworkSettings,
} from '../../../services/tcloudNetworkSettings';
import { listTrash, restoreTrash, restoreTrashMany, type ProTrashItem } from '../../../services/tcloudProductivity';
import { purgeTrash } from '../../../services/tcloudSyncMaturity';
import { formatBytes } from '../../../utils';

type Tab = 'sync' | 'conflicts' | 'storage' | 'trash';

interface Conflict {
  id: number;
  file_id: number;
  local_hash: string | null;
  remote_hash: string | null;
  resolution: string;
  created_at: number;
  file_name?: string | null;
}

interface SyncActivity {
  id: number; file_id: number; file_name: string; file_path: string;
  operation: string; status: string; error: string | null;
  created_at: number; updated_at: number; bytes_total: number; bytes_transferred: number;
  speed_bps: number; eta_seconds: number | null; paused: boolean; cancel_requested: boolean; priority: number;
}
interface SyncCenter {
  pending: number; running: number; failed: number; completed: number;
  uploads: number; downloads: number; deletes: number; recent: SyncActivity[];
}
interface TransferLimits { max_concurrent: number; }
interface GlobalPauseState { paused: boolean; until: number | null; }

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

function formatSpeed(value: number) {
  if (!value || value <= 0) return '0 B/s';
  return `${formatBytes(value)}/s`;
}

function formatEta(value: number | null | undefined) {
  if (value == null || value < 0) return '—';
  if (value < 60) return `${value}s`;
  const minutes = Math.floor(value / 60);
  const seconds = value % 60;
  if (minutes < 60) return `${minutes}m ${seconds}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

function priorityLabel(value: number) {
  if (value >= 80) return 'Alta';
  if (value <= -50) return 'Baixa';
  return 'Normal';
}

function operationLabel(value: string) {
  const key = value.toLowerCase();
  if (key.includes('upload') || key.includes('create')) return 'Enviando para a nuvem';
  if (key.includes('download') || key.includes('fetch') || key.includes('hydrate')) return 'Baixando para o computador';
  if (key.includes('delete') || key.includes('trash')) return 'Removendo';
  if (key.includes('move')) return 'Movendo';
  if (key.includes('rename')) return 'Renomeando';
  if (key.includes('copy')) return 'Copiando';
  if (key.includes('refresh') || key.includes('update')) return 'Atualizando';
  return 'Sincronizando';
}

function statusLabel(value: string) {
  switch (value) {
    case 'pending': return 'Aguardando';
    case 'running': return 'Em andamento';
    case 'paused': return 'Pausado';
    case 'completed': return 'Concluído';
    case 'failed': return 'Com problema';
    case 'cancelled': return 'Cancelado';
    default: return value;
  }
}

function pauseDescription(state: GlobalPauseState | null) {
  if (!state?.paused) return 'Sincronização ativa';
  if (state.until === -1) return 'Sincronização pausada';
  if (state.until && state.until > 0) return `Pausada até ${new Date(state.until * 1000).toLocaleString()}`;
  return 'Sincronização pausada';
}

export function TCloudOperationsCenterModal({ isOpen, onClose }: Props) {
  const [tab, setTab] = useState<Tab>('sync');
  const [conflicts, setConflicts] = useState<Conflict[]>([]);
  const [trash, setTrash] = useState<ProTrashItem[]>([]);
  const [sync, setSync] = useState<SyncCenter | null>(null);
  const [cache, setCache] = useState<TCloudCacheStatus | null>(null);
  const [limits, setLimits] = useState<TransferLimits>({ max_concurrent: 4 });
  const [network, setNetwork] = useState<TCloudNetworkSettings>({ economy_mode: false, upload_limit_bps: 0, download_limit_bps: 0 });
  const [globalPause, setGlobalPause] = useState<GlobalPauseState | null>(null);
  const [busy, setBusy] = useState(false);
  const [cacheLimitGb, setCacheLimitGb] = useState(10);

  const refresh = useCallback(async () => {
    if (!isOpen) return;
    setBusy(true);
    try {
      const [nextSync, nextConflicts, nextCache, nextLimits, nextNetwork, nextPause, nextTrash] = await Promise.all([
        invoke<SyncCenter>('tcloud_get_sync_center').catch(() => null),
        invoke<Conflict[]>('tcloud_list_conflicts').catch(() => []),
        getTCloudCacheStatus().catch(() => null),
        invoke<TransferLimits>('tcloud_get_transfer_limits').catch(() => ({ max_concurrent: 4 })),
        getTCloudNetworkSettings().catch(() => ({ economy_mode: false, upload_limit_bps: 0, download_limit_bps: 0 })),
        invoke<GlobalPauseState>('tcloud_get_global_pause').catch(() => ({ paused: false, until: null })),
        listTrash().catch(() => []),
      ]);
      setSync(nextSync);
      setConflicts(nextConflicts);
      setCache(nextCache);
      setLimits(nextLimits);
      setNetwork(nextNetwork);
      setGlobalPause(nextPause);
      setTrash(nextTrash);
      if (nextCache?.limit_bytes) {
        setCacheLimitGb(Math.max(1, Math.round(nextCache.limit_bytes / 1024 / 1024 / 1024)));
      }
    } finally {
      setBusy(false);
    }
  }, [isOpen]);

  useEffect(() => { void refresh(); }, [refresh]);

  useEffect(() => {
    if (!isOpen || tab !== 'sync') return;
    let disposed = false;
    let timer: number | null = null;
    const poll = async () => {
      try {
        const [nextSync, nextPause] = await Promise.all([
          invoke<SyncCenter>('tcloud_get_sync_center'),
          invoke<GlobalPauseState>('tcloud_get_global_pause'),
        ]);
        if (!disposed) { setSync(nextSync); setGlobalPause(nextPause); }
      } catch {
        // Keep the last known state; a busy database should not create an IPC pile-up.
      } finally {
        if (!disposed) timer = window.setTimeout(() => void poll(), 1800);
      }
    };
    void poll();
    return () => {
      disposed = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [isOpen, tab]);

  const cachePercent = useMemo(() => {
    if (!cache || cache.limit_bytes <= 0) return 0;
    return Math.min(100, Math.round((cache.current_bytes / cache.limit_bytes) * 100));
  }, [cache]);

  const running = useMemo(() => (sync?.recent ?? []).filter((item) => item.status === 'running'), [sync]);
  const aggregateSpeed = useMemo(() => running.reduce((sum, item) => sum + Math.max(0, item.speed_bps), 0), [running]);
  const nextEta = useMemo(() => running.map((item) => item.eta_seconds).filter((value): value is number => value != null && value >= 0).sort((a, b) => a - b)[0] ?? null, [running]);

  const syncHeadline = useMemo(() => {
    if ((sync?.failed ?? 0) > 0) return { tone: 'danger', title: 'Alguns arquivos precisam de atenção', detail: `${sync?.failed} ${sync?.failed === 1 ? 'item apresentou' : 'itens apresentaram'} problema.` };
    if (globalPause?.paused) return { tone: 'paused', title: 'Sincronização pausada', detail: 'Novas alterações ficarão aguardando até você retomar.' };
    if ((sync?.running ?? 0) + (sync?.pending ?? 0) > 0) return { tone: 'busy', title: 'Sincronizando seus arquivos', detail: `${(sync?.running ?? 0) + (sync?.pending ?? 0)} ${((sync?.running ?? 0) + (sync?.pending ?? 0)) === 1 ? 'arquivo na fila' : 'arquivos na fila'}.` };
    return { tone: 'ok', title: 'Tudo atualizado', detail: 'Não há alterações esperando sincronização.' };
  }, [globalPause, sync]);

  if (!isOpen) return null;

  const resolve = async (id: number, resolution: 'keep_local' | 'keep_remote' | 'keep_both') => {
    await invoke('tcloud_resolve_conflict', { conflictId: id, resolution });
    await refresh();
  };

  const applyCacheLimit = async () => {
    setBusy(true);
    try {
      const bytes = cacheLimitGb * 1024 * 1024 * 1024;
      setCache(await setTCloudCacheLimit(bytes));
    } finally { setBusy(false); }
  };

  const cleanupCache = async () => {
    setBusy(true);
    try { setCache(await cleanupTCloudCache()); } finally { setBusy(false); }
  };

  const setPause = async (until: number) => {
    setGlobalPause(await invoke<GlobalPauseState>('tcloud_set_global_pause', { until }));
    await refresh();
  };

  const pauseForHours = (hours: number) => setPause(Math.floor(Date.now() / 1000) + hours * 3600);
  const pauseUntilTomorrow = () => {
    const tomorrow = new Date();
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(8, 0, 0, 0);
    return setPause(Math.floor(tomorrow.getTime() / 1000));
  };

  const setConcurrency = async (value: number) => {
    setLimits(await invoke<TransferLimits>('tcloud_set_transfer_limits', { maxConcurrent: value }));
  };

  const setBandwidth = async (direction: 'up' | 'down', mbps: number) => {
    const bps = mbps <= 0 ? 0 : Math.round(mbps * 1024 * 1024);
    const next = await setTCloudNetworkSettings(
      network.economy_mode,
      direction === 'up' ? bps : network.upload_limit_bps,
      direction === 'down' ? bps : network.download_limit_bps,
    );
    setNetwork(next);
  };

  const setPriority = async (item: SyncActivity, priority: 'high' | 'normal' | 'low') => {
    if (item.id <= 0) return;
    await invoke('tcloud_set_job_priority', { jobId: item.id, priority });
    await refresh();
  };

  const restoreTrashItem = async (item: ProTrashItem) => {
    setBusy(true);
    try {
      await restoreTrash(item.id);
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      await refresh();
    } finally { setBusy(false); }
  };

  const deleteTrashItem = async (item: ProTrashItem) => {
    if (!window.confirm(`Excluir “${item.name}” definitivamente? Essa ação não pode ser desfeita.`)) return;
    setBusy(true);
    try {
      await purgeTrash(item.id);
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      await refresh();
    } finally { setBusy(false); }
  };

  const restoreAllTrash = async () => {
    if (trash.length === 0) return;
    setBusy(true);
    try {
      const result = await restoreTrashMany(trash.map((item) => item.id));
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      await refresh();
      if (result.failed > 0) {
        window.alert(`${result.restored} restaurado(s). ${result.failed} item(ns) precisam de atenção.`);
      }
    } finally { setBusy(false); }
  };

  const emptyTrash = async () => {
    if (trash.length === 0) return;
    if (!window.confirm(`Excluir definitivamente ${trash.length} ${trash.length === 1 ? 'arquivo' : 'arquivos'} da Lixeira? Essa ação não pode ser desfeita.`)) return;
    setBusy(true);
    try {
      await purgeTrash(null);
      window.dispatchEvent(new Event('tcloud-trash-changed'));
      await refresh();
    } finally { setBusy(false); }
  };

  return (
    <div className="fixed inset-0 z-[250] flex items-center justify-center bg-app-overlay p-4 backdrop-blur-sm" onClick={onClose}>
      <div className="tcloud-ops-modal" onClick={(event) => event.stopPropagation()}>
        <header className="tcloud-ops-header">
          <div><span>TCLOUD</span><h2>Sincronização</h2><p>Acompanhe o que está acontecendo com seus arquivos.</p></div>
          <div className="flex items-center gap-1">
            <button type="button" onClick={() => void refresh()} className="tcloud-ops-icon" title="Atualizar"><RefreshCw className={`h-4 w-4 ${busy ? 'animate-spin' : ''}`} /></button>
            <button type="button" onClick={onClose} className="tcloud-ops-icon" title="Fechar"><X className="h-4 w-4" /></button>
          </div>
        </header>

        <nav className="tcloud-ops-tabs">
          <button className={tab === 'sync' ? 'is-active' : ''} onClick={() => setTab('sync')}><RefreshCw className="h-4 w-4" /> Atividade{Boolean((sync?.pending ?? 0) + (sync?.running ?? 0) + (sync?.failed ?? 0)) && <b>{(sync?.pending ?? 0) + (sync?.running ?? 0) + (sync?.failed ?? 0)}</b>}</button>
          <button className={tab === 'conflicts' ? 'is-active' : ''} onClick={() => setTab('conflicts')}><AlertTriangle className="h-4 w-4" /> Problemas{conflicts.length > 0 && <b>{conflicts.length}</b>}</button>
          <button className={tab === 'storage' ? 'is-active' : ''} onClick={() => setTab('storage')}><Database className="h-4 w-4" /> Espaço</button>
          <button className={tab === 'trash' ? 'is-active' : ''} onClick={() => setTab('trash')}><Trash2 className="h-4 w-4" /> Lixeira{trash.length > 0 && <b>{trash.length}</b>}</button>
        </nav>

        <main className="tcloud-ops-body">
          {tab === 'sync' && (
            <section>
              <div className={`tcloud-simple-sync-banner is-${syncHeadline.tone}`}>
                <div>{syncHeadline.tone === 'ok' ? <CheckCircle2 className="h-5 w-5" /> : <RefreshCw className={`h-5 w-5 ${syncHeadline.tone === 'busy' ? 'animate-spin' : ''}`} />}</div>
                <span><strong>{syncHeadline.title}</strong><small>{syncHeadline.detail}</small></span>
              </div>

              <div className="tcloud-cache-summary tcloud-simple-summary">
                <div><span>Aguardando</span><strong>{sync?.pending ?? 0}</strong></div>
                <div><span>Transferindo</span><strong>{sync?.running ?? 0}</strong></div>
                <div><span>Velocidade</span><strong>{formatSpeed(aggregateSpeed)}</strong></div>
                <div><span>Tempo restante</span><strong>{formatEta(nextEta)}</strong></div>
              </div>

              <div className="tcloud-cache-card">
                <div className="tcloud-ops-section-title">
                  <div><span>CONTROLE</span><h3>{pauseDescription(globalPause)}</h3></div>
                  {globalPause?.paused ? <Pause className="h-5 w-5 text-app-warning"/> : <Play className="h-5 w-5 text-app-success"/>}
                </div>
                <div className="tcloud-cache-actions">
                  {globalPause?.paused
                    ? <button onClick={() => void setPause(0)}><Play className="h-3.5 w-3.5"/> Retomar</button>
                    : <button onClick={() => void setPause(-1)}><Pause className="h-3.5 w-3.5"/> Pausar</button>}
                  <button onClick={() => void pauseForHours(1)}><Timer className="h-3.5 w-3.5"/> Por 1 hora</button>
                  <button onClick={() => void pauseForHours(2)}><Timer className="h-3.5 w-3.5"/> Por 2 horas</button>
                  <button onClick={() => void pauseUntilTomorrow()}><Timer className="h-3.5 w-3.5"/> Até amanhã</button>
                </div>

                <details className="tcloud-simple-details">
                  <summary>Ajustes de velocidade</summary>
                  <p>Use somente se quiser limitar a internet usada pelo TCloud.</p>
                  <div className="tcloud-cache-summary">
                    <label><span>Arquivos ao mesmo tempo</span><strong>{limits.max_concurrent}</strong>
                      <select value={limits.max_concurrent} onChange={(e) => void setConcurrency(Number(e.target.value))}>
                        {[1,2,4,6,8].map((value) => <option key={value} value={value}>{value}</option>)}
                      </select>
                    </label>
                    <label><span>Envio</span><strong>{network.upload_limit_bps ? formatSpeed(network.upload_limit_bps) : 'Sem limite'}</strong>
                      <select value={network.upload_limit_bps ? Math.round(network.upload_limit_bps / 1024 / 1024) : 0} onChange={(e) => void setBandwidth('up', Number(e.target.value))}>
                        {[0,1,5,10,25,50].map((value)=><option key={value} value={value}>{value === 0 ? 'Sem limite' : `${value} MB/s`}</option>)}
                      </select>
                    </label>
                    <label><span>Download</span><strong>{network.download_limit_bps ? formatSpeed(network.download_limit_bps) : 'Sem limite'}</strong>
                      <select value={network.download_limit_bps ? Math.round(network.download_limit_bps / 1024 / 1024) : 0} onChange={(e) => void setBandwidth('down', Number(e.target.value))}>
                        {[0,1,5,10,25,50].map((value)=><option key={value} value={value}>{value === 0 ? 'Sem limite' : `${value} MB/s`}</option>)}
                      </select>
                    </label>
                  </div>
                </details>
              </div>

              <div className="tcloud-cache-actions">
                <button disabled={busy || !(sync?.completed)} onClick={async()=>{await invoke('tcloud_clear_completed_jobs');await refresh();}}>Limpar histórico concluído</button>
                <button disabled={busy} onClick={()=>void refresh()}>Atualizar</button>
              </div>

              <div className="tcloud-conflict-list">
                {(sync?.recent ?? []).slice(0,80).map((item)=>{
                  const percent = item.bytes_total > 0 ? Math.min(100, Math.round((item.bytes_transferred / item.bytes_total) * 100)) : 0;
                  return (
                    <article key={item.id} className="tcloud-conflict-card">
                      <div className="min-w-0 flex-1">
                        <strong>{item.file_name}</strong>
                        <span>{operationLabel(item.operation)} · {statusLabel(item.status)}{priorityLabel(item.priority) === 'Alta' ? ' · Prioridade alta' : ''}</span>
                        {item.bytes_total > 0 && <div className="tcloud-cache-meter"><span style={{ width: `${percent}%` }} /></div>}
                        <small>{item.bytes_total > 0 ? `${percent}% · ${formatSpeed(item.speed_bps)} · ${formatEta(item.eta_seconds)} restantes` : item.error || 'Aguardando sincronização'}</small>
                        {item.error && <small className="tcloud-simple-error">{item.error}</small>}
                      </div>
                      <div className="tcloud-conflict-actions">
                        {item.id > 0 && ['pending','running','paused'].includes(item.status) && priorityLabel(item.priority) !== 'Alta' && <button title="Fazer este arquivo passar na frente" onClick={() => void setPriority(item,'high')}><Gauge className="h-3.5 w-3.5"/> Priorizar</button>}
                        {item.id > 0 && ['pending','running','paused'].includes(item.status) && priorityLabel(item.priority) === 'Alta' && <button title="Voltar à prioridade normal" onClick={() => void setPriority(item,'normal')}>Prioridade normal</button>}
                        {item.id > 0 && item.status === 'running' && <button onClick={async()=>{await invoke('tcloud_pause_sync_job',{jobId:item.id});await refresh();}}><Pause className="h-3.5 w-3.5"/> Pausar</button>}
                        {item.id > 0 && item.status === 'paused' && <button onClick={async()=>{await invoke('tcloud_resume_sync_job',{jobId:item.id});await refresh();}}><Play className="h-3.5 w-3.5"/> Retomar</button>}
                        {item.id > 0 && ['pending','running','paused'].includes(item.status) && <button className="is-danger" onClick={async()=>{await invoke('tcloud_cancel_sync_job',{jobId:item.id});await refresh();}}>Cancelar</button>}
                        {item.status === 'failed' && <button onClick={async()=>{await invoke('tcloud_retry_sync_job',{jobId:item.id});await refresh();}}>Tentar novamente</button>}
                      </div>
                    </article>
                  );
                })}
                {(sync?.recent?.length ?? 0) === 0 && <div className="tcloud-ops-empty"><CheckCircle2 className="h-8 w-8 text-app-success"/><strong>Tudo atualizado</strong><span>Não há arquivos esperando sincronização.</span></div>}
              </div>
            </section>
          )}

          {tab === 'conflicts' && (
            <section>
              <div className="tcloud-ops-section-title"><div><span>PRECISA DE VOCÊ</span><h3>Arquivos que precisam de uma escolha</h3></div><AlertTriangle className="h-5 w-5 text-app-warning" /></div>
              {conflicts.length === 0 ? (
                <div className="tcloud-ops-empty"><CheckCircle2 className="h-8 w-8 text-app-success" /><strong>Nenhum problema pendente</strong><span>O computador e a nuvem estão com as mesmas versões.</span></div>
              ) : (
                <div className="tcloud-conflict-list">
                  {conflicts.map((item) => (
                    <article key={item.id} className="tcloud-conflict-card">
                      <div className="min-w-0"><strong>{item.file_name || `Arquivo #${item.file_id}`}</strong><span>Este arquivo foi alterado em mais de um lugar.</span><small>{new Date(item.created_at * 1000).toLocaleString()}</small></div>
                      <div className="tcloud-conflict-actions"><button onClick={() => void resolve(item.id, 'keep_local')}>Usar versão do computador</button><button onClick={() => void resolve(item.id, 'keep_remote')}>Usar versão da nuvem</button><button onClick={() => void resolve(item.id, 'keep_both')}>Guardar as duas</button></div>
                    </article>
                  ))}
                </div>
              )}
            </section>
          )}

          {tab === 'storage' && (
            <section>
              <div className="tcloud-ops-section-title"><div><span>NO COMPUTADOR</span><h3>Espaço usado pelo TCloud</h3></div><Database className="h-5 w-5 text-app-accent" /></div>
              <div className="tcloud-cache-card">
                <p className="tcloud-simple-description">O TCloud guarda arquivos recentes no computador para abrir mais rápido. Os arquivos continuam seguros na nuvem.</p>
                <div className="tcloud-cache-summary"><div><span>Usado</span><strong>{formatBytes(cache?.current_bytes ?? 0)}</strong></div><div><span>Limite</span><strong>{formatBytes(cache?.limit_bytes ?? 0)}</strong></div><div><span>Ocupação</span><strong>{cachePercent}%</strong></div></div>
                <div className="tcloud-cache-meter"><span style={{ width: `${cachePercent}%` }} /></div>
                <label className="tcloud-cache-limit"><span>Máximo que o TCloud pode usar neste computador</span><div><input type="number" min={1} max={500} value={cacheLimitGb} onChange={(e) => setCacheLimitGb(Number(e.target.value))} /><b>GB</b></div></label>
                <div className="tcloud-cache-actions"><button onClick={() => void applyCacheLimit()} disabled={busy}>Salvar</button><button onClick={() => void cleanupCache()} disabled={busy}>Liberar espaço extra</button></div>
              </div>
            </section>
          )}

          {tab === 'trash' && (
            <section>
              <div className="tcloud-ops-section-title"><div><span>ARQUIVOS EXCLUÍDOS</span><h3>Lixeira</h3></div><ArchiveRestore className="h-5 w-5 text-app-warning" /></div>
              {trash.length === 0 ? (
                <div className="tcloud-ops-empty"><Trash2 className="h-8 w-8 text-app-text-tertiary"/><strong>A lixeira está vazia</strong><span>Arquivos excluídos aparecem aqui antes da remoção definitiva.</span></div>
              ) : (
                <>
                  <div className="tcloud-trash-actions">
                    <button type="button" onClick={() => void restoreAllTrash()} disabled={busy}><ArchiveRestore className="h-3.5 w-3.5"/> Restaurar tudo</button>
                    <button type="button" className="is-danger" onClick={() => void emptyTrash()} disabled={busy}><Trash2 className="h-3.5 w-3.5"/> Esvaziar lixeira</button>
                  </div>
                  <div className="tcloud-conflict-list">
                    {trash.map((item) => (
                      <article key={item.id} className="tcloud-conflict-card">
                        <div className="min-w-0 flex-1">
                          <strong>{item.name}</strong>
                          <span>{formatBytes(item.size)} · Excluído em {new Date(item.deleted_at * 1000).toLocaleString()}</span>
                          {item.relative_path && <small>Local original: {item.relative_path}</small>}
                          {item.expires_at && <small>Remoção automática: {new Date(item.expires_at * 1000).toLocaleDateString()}</small>}
                        </div>
                        <div className="tcloud-conflict-actions"><button onClick={() => void restoreTrashItem(item)}><ArchiveRestore className="h-3.5 w-3.5"/> Restaurar</button><button className="is-danger" onClick={() => void deleteTrashItem(item)}>Excluir definitivamente</button></div>
                      </article>
                    ))}
                  </div>
                </>
              )}
            </section>
          )}
        </main>
      </div>
    </div>
  );
}
